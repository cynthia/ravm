/*
 * Copyright (c) 2026, Alliance for Open Media. All rights reserved
 *
 * This source code is subject to the terms of the BSD 3-Clause Clear License
 * and the Alliance for Open Media Patent License 1.0. If the BSD 3-Clause Clear
 * License was not distributed with this source code in the LICENSE file, you
 * can obtain it at aomedia.org/license/software-license/bsd-3-c-c/.  If the
 * Alliance for Open Media Patent License 1.0 was not distributed with this
 * source code in the PATENTS file, you can obtain it at
 * aomedia.org/license/patent-license/.
 */

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include "third_party/googletest/src/googletest/include/gtest/gtest.h"
#include "test/codec_factory.h"
#include "test/encode_test_driver.h"
#include "test/y4m_video_source.h"
#include "test/util.h"
#include "av2/common/enums.h"
#include "av2/common/obu_util.h"

namespace {

// ---------------------------------------------------------------------------
// Pruning decisions are based on OBU header metadata (temporal layer ID,
// embedded layer ID, OBU type)
// ---------------------------------------------------------------------------

// Metadata extracted from a packet's first VCL OBU header.
struct PacketInfo {
  int tlayer_id;      // temporal layer from OBU header
  int mlayer_id;      // embedded (spatial) layer from OBU header
  OBU_TYPE vcl_type;  // OBU type of the first VCL OBU (e.g. OBU_RAS_FRAME)
  bool has_vcl;       // true if the packet contains at least one VCL OBU
};

// Parse the first VCL OBU and populate info.
static PacketInfo GetPacketInfo(const std::vector<uint8_t> &data) {
  PacketInfo info = {};
  info.has_vcl = false;
  const uint8_t *ptr = data.data();
  size_t remaining = data.size();
  while (remaining > 0) {
    ObuHeader hdr;
    size_t payload_size = 0;
    size_t bytes_read = 0;
    if (avm_read_obu_header_and_size(ptr, remaining, &hdr, &payload_size,
                                     &bytes_read) != AVM_CODEC_OK)
      break;
    if (is_single_tile_vcl_obu(hdr.type) || is_multi_tile_vcl_obu(hdr.type)) {
      info.tlayer_id = hdr.obu_tlayer_id;
      info.mlayer_id = hdr.obu_mlayer_id;
      info.vcl_type = hdr.type;
      info.has_vcl = true;
      break;
    }
    ptr += bytes_read + payload_size;
    remaining -= bytes_read + payload_size;
  }
  return info;
}

// Returns true if the OBU type is a decoder reset point (KEY, OLK, or RAS).
static bool IsResetPoint(OBU_TYPE type) {
  return type == OBU_CLOSED_LOOP_KEY || type == OBU_OPEN_LOOP_KEY ||
         type == OBU_RAS_FRAME;
}

// Deep-copy an avm_image_t. Caller owns the returned image and must call
// avm_img_free() on it.
static avm_image_t *CopyImage(const avm_image_t *src) {
  avm_image_t *dst = avm_img_alloc(NULL, src->fmt, src->d_w, src->d_h, 1);
  if (!dst) return NULL;
  dst->bit_depth = src->bit_depth;
  dst->monochrome = src->monochrome;
  const int num_planes = src->monochrome ? 1 : 3;
  for (int plane = 0; plane < num_planes; ++plane) {
    const int w = avm_img_plane_width(src, plane);
    const int h = avm_img_plane_height(src, plane);
    const int bytes_per_sample = (src->fmt & AVM_IMG_FMT_HIGHBITDEPTH) ? 2 : 1;
    const uint8_t *src_row = src->planes[plane];
    uint8_t *dst_row = dst->planes[plane];
    for (int y = 0; y < h; ++y) {
      memcpy(dst_row, src_row, w * bytes_per_sample);
      src_row += src->stride[plane];
      dst_row += dst->stride[plane];
    }
  }
  return dst;
}

// Compare two avm_image_t pixel-by-pixel. Returns true if identical.
static bool ImagesMatch(const avm_image_t *a, const avm_image_t *b) {
  if (a->fmt != b->fmt || a->d_w != b->d_w || a->d_h != b->d_h ||
      a->monochrome != b->monochrome)
    return false;
  const int num_planes = a->monochrome ? 1 : 3;
  for (int plane = 0; plane < num_planes; ++plane) {
    const int w = avm_img_plane_width(a, plane);
    const int h = avm_img_plane_height(a, plane);
    const int bytes_per_sample = (a->fmt & AVM_IMG_FMT_HIGHBITDEPTH) ? 2 : 1;
    const uint8_t *row_a = a->planes[plane];
    const uint8_t *row_b = b->planes[plane];
    for (int y = 0; y < h; ++y) {
      if (memcmp(row_a, row_b, w * bytes_per_sample) != 0) return false;
      row_a += a->stride[plane];
      row_b += b->stride[plane];
    }
  }
  return true;
}

// Decode a sequence of packets and collect decoded images.
// Returns decoded images in display order.
static std::vector<avm_image_t *> DecodePackets(
    const std::vector<std::vector<uint8_t>> &packets) {
  std::vector<avm_image_t *> frames;
  avm_codec_dec_cfg_t dec_cfg;
  memset(&dec_cfg, 0, sizeof(dec_cfg));
  dec_cfg.threads = 1;
  libavm_test::AV2Decoder decoder(dec_cfg);

  for (const auto &pkt : packets) {
    avm_codec_err_t res = decoder.DecodeFrame(pkt.data(), pkt.size());
    EXPECT_EQ(AVM_CODEC_OK, res) << decoder.DecodeError();
    if (res != AVM_CODEC_OK) break;
    libavm_test::DxDataIterator iter = decoder.GetDxData();
    const avm_image_t *img;
    while ((img = iter.Next()) != NULL) {
      frames.push_back(CopyImage(img));
    }
  }
  // Flush.
  decoder.DecodeFrame(NULL, 0);
  libavm_test::DxDataIterator final_iter = decoder.GetDxData();
  const avm_image_t *img;
  while ((img = final_iter.Next()) != NULL) {
    frames.push_back(CopyImage(img));
  }
  return frames;
}

static void FreeImages(std::vector<avm_image_t *> &images) {
  for (auto *img : images) avm_img_free(img);
  images.clear();
}

// Prune for multi-temporal-layer tests: retain TL0 and remove prune
// pictures between first KEY and first RAS.
//
// Keeps a packet if:
//   - It is non-VCL (sequence header, etc.), OR
//   - Its temporal layer is 0 AND it is either a reset point (CLK/OLK/RAS)
//     or appears after the first RAS has been retained.
static std::vector<std::vector<uint8_t>> PruneTL0RetainAfterFirstRAS(
    const std::vector<std::vector<uint8_t>> &packets) {
  std::vector<std::vector<uint8_t>> out;
  bool seen_ras = false;
  for (const auto &pkt : packets) {
    const PacketInfo info = GetPacketInfo(pkt);
    if (!info.has_vcl) {
      out.push_back(pkt);
      continue;
    }
    if (info.tlayer_id != 0) continue;
    if (info.vcl_type == OBU_RAS_FRAME) seen_ras = true;
    if (IsResetPoint(info.vcl_type) || seen_ras) {
      out.push_back(pkt);
    }
  }
  return out;
}

// Prune for random-access test: Remove pictures between OBU_CLOSED_LOOP_KEY and
// first OBU_RAS_FRAME
static std::vector<std::vector<uint8_t>> PruneFirstGoP(
    const std::vector<std::vector<uint8_t>> &packets) {
  std::vector<std::vector<uint8_t>> out;
  int ras_count = 0;
  for (const auto &pkt : packets) {
    const PacketInfo info = GetPacketInfo(pkt);
    if (!info.has_vcl) {
      out.push_back(pkt);
      continue;
    }
    if (info.vcl_type == OBU_CLOSED_LOOP_KEY) {
      out.push_back(pkt);
      continue;
    }
    if (info.vcl_type == OBU_RAS_FRAME) ++ras_count;
    // Keep everything from the second RAS onward.
    if (ras_count >= 2) {
      out.push_back(pkt);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// RAS Frame Pruning Test
//
// Encodes 20 frames with 2 temporal layers and RAS frames (sframe_type=1) at
// TL0 distance 4.  With 2TL the TL0 frames are at even frame numbers:
// 0,2,4,6,8,...  sframe_dist=8 places the first RAS at frame 8 (the 5th TL0
// frame, i.e. TL0 index 4 — distance 4 in TL0 from the KEY at index 0).
//
// Pruning based on OBU header metadata (temporal layer and OBU type):
//   1. Remove all non-TL0 packets (tlayer_id != 0).
//   2. Among TL0, remove non-reset-point frames before the first RAS.
//
// Retained: KEY(0), RAS(8), and TL0 frames after RAS: 10, 12, 14, 16, 18.
//
// The test:
//   1. Encodes and stores each packet individually.
//   2. Decodes all packets to get original frames.
//   3. Prunes packets based on tlayer_id and OBU type.
//   4. Decodes pruned packets to get pruned frames.
//   5. Verifies retained frames are pixel-identical.
// ---------------------------------------------------------------------------
class MultiLayer2TemporalDecodeBaseOnlyRASPruningTest
    : public ::libavm_test::CodecTestWithParam<int>,
      public ::libavm_test::EncoderTest {
 protected:
  MultiLayer2TemporalDecodeBaseOnlyRASPruningTest()
      : EncoderTest(GET_PARAM(0)), speed_(GET_PARAM(1)) {}
  ~MultiLayer2TemporalDecodeBaseOnlyRASPruningTest() override {}

  void SetUp() override {
    InitializeConfig();
    passes_ = 1;
    cfg_.rc_end_usage = AVM_Q;
    cfg_.rc_min_quantizer = 210;
    cfg_.rc_max_quantizer = 210;
    cfg_.g_threads = 2;
    cfg_.g_profile = MAIN_420_10;
    cfg_.g_lag_in_frames = 0;
    cfg_.g_bit_depth = AVM_BITS_8;
    cfg_.enable_ops = 1;
    cfg_.enable_lcr = 1;
    cfg_.sframe_dist = 8;
    cfg_.sframe_mode = 0;
    cfg_.sframe_type = 1;
    num_temporal_layers_ = 2;
    num_embedded_layers_ = 1;
    layer_frame_cnt_ = 0;
    temporal_layer_id_ = 0;
  }

  int GetNumEmbeddedLayers() override { return num_embedded_layers_; }

  void PreEncodeFrameHook(::libavm_test::VideoSource *video,
                          ::libavm_test::Encoder *encoder) override {
    (void)video;
    frame_flags_ = 0;
    if (layer_frame_cnt_ == 0) {
      encoder->Control(AVME_SET_CPUUSED, speed_);
      encoder->Control(AVME_SET_NUMBER_MLAYERS, num_embedded_layers_);
      encoder->Control(AVME_SET_NUMBER_TLAYERS, num_temporal_layers_);
      encoder->Control(AVME_SET_MLAYER_ID, 0);
      encoder->Control(AVME_SET_TLAYER_ID, 0);
      encoder->Control(AV2E_SET_ENABLE_EXPLICIT_REF_FRAME_MAP, 1);
    }
    if (layer_frame_cnt_ % 2 == 0) {
      temporal_layer_id_ = 0;
      encoder->Control(AVME_SET_TLAYER_ID, 0);
    } else {
      temporal_layer_id_ = 1;
      encoder->Control(AVME_SET_TLAYER_ID, 1);
    }
    layer_frame_cnt_++;
  }

  bool HandleDecodeResult(const avm_codec_err_t res_dec,
                          libavm_test::Decoder *decoder) override {
    EXPECT_EQ(AVM_CODEC_OK, res_dec) << decoder->DecodeError();
    return AVM_CODEC_OK == res_dec;
  }

  void FramePktHook(const avm_codec_cx_pkt_t *pkt,
                    ::libavm_test::DxDataIterator *dec_iter) override {
    (void)dec_iter;
    if (pkt->kind != AVM_CODEC_CX_FRAME_PKT) return;
    const uint8_t *buf = static_cast<const uint8_t *>(pkt->data.frame.buf);
    const size_t sz = pkt->data.frame.sz;
    packets_.emplace_back(buf, buf + sz);
  }

  int speed_;
  int num_temporal_layers_;
  int num_embedded_layers_;
  int temporal_layer_id_;
  int layer_frame_cnt_;
  std::vector<std::vector<uint8_t>> packets_;
};

TEST_P(MultiLayer2TemporalDecodeBaseOnlyRASPruningTest, PruneBetweenKeyAndRAS) {
  ::libavm_test::Y4mVideoSource video("park_joy_90p_8_420.y4m", 0, 20);
  ASSERT_NO_FATAL_FAILURE(RunLoop(&video));
  ASSERT_FALSE(packets_.empty()) << "No packets produced";

  // Decode all packets to get original frames.
  std::vector<avm_image_t *> orig_frames = DecodePackets(packets_);
  ASSERT_EQ(static_cast<int>(orig_frames.size()),
            static_cast<int>(packets_.size()))
      << "Original decode frame count mismatch";

  // Prune: keep TL0 only, remove non-reset TL0 frames before first RAS.
  // Retained: KEY(0), RAS(8), and TL0 frames after RAS: 10, 12, 14, 16, 18.
  std::vector<std::vector<uint8_t>> pruned =
      PruneTL0RetainAfterFirstRAS(packets_);

  // Build retained mapping using the same filter logic.
  std::vector<int> retained_indices;
  {
    bool seen_ras = false;
    for (int i = 0; i < static_cast<int>(packets_.size()); ++i) {
      const PacketInfo info = GetPacketInfo(packets_[i]);
      if (!info.has_vcl) continue;  // non-VCL don't produce decoded frames
      if (info.tlayer_id != 0) continue;
      if (info.vcl_type == OBU_RAS_FRAME) seen_ras = true;
      if (IsResetPoint(info.vcl_type) || seen_ras) {
        retained_indices.push_back(i);
      }
    }
  }

  std::vector<avm_image_t *> pruned_frames = DecodePackets(pruned);
  ASSERT_EQ(pruned_frames.size(), retained_indices.size())
      << "Pruned decode frame count mismatch";

  for (size_t i = 0; i < pruned_frames.size(); ++i) {
    ASSERT_TRUE(ImagesMatch(orig_frames[retained_indices[i]], pruned_frames[i]))
        << "Pixel mismatch at pruned frame " << i << " (original picture "
        << retained_indices[i] << ")";
  }

  FreeImages(orig_frames);
  FreeImages(pruned_frames);
}

AV2_INSTANTIATE_TEST_SUITE(MultiLayer2TemporalDecodeBaseOnlyRASPruningTest,
                           ::testing::Values(5));

// ---------------------------------------------------------------------------
// RAS Frame Pruning Test — 3 Temporal Layers, Decode Base Only
//
// Encodes 28 frames with 3 temporal layers, 1 embedded layer, explicit ref
// frame map, and RAS frames (sframe_type=1) at TL0 distance 4.
//
// With 3TL the temporal layer assignment is:
//   TL0: frames 0, 4, 8, 12, 16, 20, 24  (layer_frame_cnt_ % 4 == 0)
//   TL1: frames 2, 6, 10, 14, 18, 22, 26 (layer_frame_cnt_ % 2 == 0, not %4)
//   TL2: frames 1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27 (odd)
//
// sframe_dist=16 places the first RAS at frame 16 (the 5th TL0 frame,
// i.e. TL0 index 4 — distance 4 in TL0 from the KEY at index 0).
//
// Pruning based on OBU header metadata (temporal layer and OBU type):
//   - Remove all non-TL0 packets (tlayer_id != 0).
//   - Among TL0, remove non-reset-point frames before the first RAS.
//
// Retained pictures: 0 (KEY), 16 (RAS), 20, 24 (4 TL0 frames).
// ---------------------------------------------------------------------------
class MultiLayer3TemporalDecodeBaseOnlyRASPruningTest
    : public ::libavm_test::CodecTestWithParam<int>,
      public ::libavm_test::EncoderTest {
 protected:
  MultiLayer3TemporalDecodeBaseOnlyRASPruningTest()
      : EncoderTest(GET_PARAM(0)), speed_(GET_PARAM(1)) {}
  ~MultiLayer3TemporalDecodeBaseOnlyRASPruningTest() override {}

  void SetUp() override {
    InitializeConfig();
    passes_ = 1;
    cfg_.rc_end_usage = AVM_Q;
    cfg_.rc_min_quantizer = 210;
    cfg_.rc_max_quantizer = 210;
    cfg_.g_threads = 2;
    cfg_.g_profile = MAIN_420_10;
    cfg_.g_lag_in_frames = 0;
    cfg_.g_bit_depth = AVM_BITS_8;
    cfg_.enable_ops = 1;
    cfg_.enable_lcr = 1;
    cfg_.sframe_dist = 16;
    cfg_.sframe_mode = 0;
    cfg_.sframe_type = 1;
    num_temporal_layers_ = 3;
    num_embedded_layers_ = 1;
    layer_frame_cnt_ = 0;
    temporal_layer_id_ = 0;
  }

  int GetNumEmbeddedLayers() override { return num_embedded_layers_; }

  void PreEncodeFrameHook(::libavm_test::VideoSource *video,
                          ::libavm_test::Encoder *encoder) override {
    (void)video;
    frame_flags_ = 0;
    if (layer_frame_cnt_ == 0) {
      encoder->Control(AVME_SET_CPUUSED, speed_);
      encoder->Control(AVME_SET_NUMBER_MLAYERS, num_embedded_layers_);
      encoder->Control(AVME_SET_NUMBER_TLAYERS, num_temporal_layers_);
      encoder->Control(AVME_SET_MLAYER_ID, 0);
      encoder->Control(AVME_SET_TLAYER_ID, 0);
      encoder->Control(AV2E_SET_ENABLE_EXPLICIT_REF_FRAME_MAP, 1);
    }
    if (layer_frame_cnt_ % 4 == 0) {
      temporal_layer_id_ = 0;
      encoder->Control(AVME_SET_TLAYER_ID, 0);
    } else if (layer_frame_cnt_ % 2 == 0) {
      temporal_layer_id_ = 1;
      encoder->Control(AVME_SET_TLAYER_ID, 1);
    } else {
      temporal_layer_id_ = 2;
      encoder->Control(AVME_SET_TLAYER_ID, 2);
    }
    layer_frame_cnt_++;
  }

  bool HandleDecodeResult(const avm_codec_err_t res_dec,
                          libavm_test::Decoder *decoder) override {
    EXPECT_EQ(AVM_CODEC_OK, res_dec) << decoder->DecodeError();
    return AVM_CODEC_OK == res_dec;
  }

  void FramePktHook(const avm_codec_cx_pkt_t *pkt,
                    ::libavm_test::DxDataIterator *dec_iter) override {
    (void)dec_iter;
    if (pkt->kind != AVM_CODEC_CX_FRAME_PKT) return;
    const uint8_t *buf = static_cast<const uint8_t *>(pkt->data.frame.buf);
    const size_t sz = pkt->data.frame.sz;
    packets_.emplace_back(buf, buf + sz);
  }

  int speed_;
  int num_temporal_layers_;
  int num_embedded_layers_;
  int temporal_layer_id_;
  int layer_frame_cnt_;
  std::vector<std::vector<uint8_t>> packets_;
};

TEST_P(MultiLayer3TemporalDecodeBaseOnlyRASPruningTest, PruneBetweenKeyAndRAS) {
  ::libavm_test::Y4mVideoSource video("park_joy_90p_8_420.y4m", 0, 28);
  ASSERT_NO_FATAL_FAILURE(RunLoop(&video));
  ASSERT_FALSE(packets_.empty()) << "No packets produced";

  // Decode all packets to get original frames.
  std::vector<avm_image_t *> orig_frames = DecodePackets(packets_);
  ASSERT_EQ(static_cast<int>(orig_frames.size()),
            static_cast<int>(packets_.size()))
      << "Original decode frame count mismatch";

  // Prune: keep TL0 only, remove non-reset TL0 frames before first RAS.
  // Retained: KEY(0), RAS(16), and TL0 frames after RAS: 20, 24.
  std::vector<std::vector<uint8_t>> pruned =
      PruneTL0RetainAfterFirstRAS(packets_);

  // Build retained mapping using the same filter logic.
  std::vector<int> retained_indices;
  {
    bool seen_ras = false;
    for (int i = 0; i < static_cast<int>(packets_.size()); ++i) {
      const PacketInfo info = GetPacketInfo(packets_[i]);
      if (!info.has_vcl) continue;
      if (info.tlayer_id != 0) continue;
      if (info.vcl_type == OBU_RAS_FRAME) seen_ras = true;
      if (IsResetPoint(info.vcl_type) || seen_ras) {
        retained_indices.push_back(i);
      }
    }
  }

  std::vector<avm_image_t *> pruned_frames = DecodePackets(pruned);
  ASSERT_EQ(pruned_frames.size(), retained_indices.size())
      << "Pruned decode frame count mismatch";

  for (size_t i = 0; i < pruned_frames.size(); ++i) {
    ASSERT_TRUE(ImagesMatch(orig_frames[retained_indices[i]], pruned_frames[i]))
        << "Pixel mismatch at pruned frame " << i << " (original picture "
        << retained_indices[i] << ")";
  }

  FreeImages(orig_frames);
  FreeImages(pruned_frames);
}

AV2_INSTANTIATE_TEST_SUITE(MultiLayer3TemporalDecodeBaseOnlyRASPruningTest,
                           ::testing::Values(5));

// ---------------------------------------------------------------------------
// Random Access RAS Pruning Test
//
// Encodes 17 frames in single-layer random access configuration with RAS
// frames enabled (sframe_type=1, sframe_dist=4, sframe_mode=2).
//
// The encoded bitstream contains 17 pictures (POC 0-16):
//   POC 0  : KEY (OBU_CLOSED_LOOP_KEY)
//   POC 8  : RAS (OBU_RAS_FRAME)
//   POC 4,2,1,3,6,5,7 : INTER (pyramid within first GoP)
//   POC 16 : RAS (OBU_RAS_FRAME)
//   POC 12,10,9,11,14,13,15 : INTER (pyramid within second GoP)
//
// Pruning based on OBU type: keep KEY, skip the entire first GoP
// (including its leading RAS in decode order), keep everything from
// the second GoP's RAS onward. The filter counts OBU_RAS_FRAME
// occurrences and retains from the second RAS.
// ---------------------------------------------------------------------------
class RandomAccessRASPruningTest
    : public ::libavm_test::CodecTestWithParam<int>,
      public ::libavm_test::EncoderTest {
 protected:
  RandomAccessRASPruningTest()
      : EncoderTest(GET_PARAM(0)), speed_(GET_PARAM(1)) {}
  ~RandomAccessRASPruningTest() override {}

  void SetUp() override {
    InitializeConfig();
    passes_ = 1;
    cfg_.rc_end_usage = AVM_Q;
    cfg_.g_threads = 1;
    cfg_.g_lag_in_frames = 19;
    cfg_.kf_min_dist = 65;
    cfg_.kf_max_dist = 65;
    cfg_.use_fixed_qp_offsets = 1;
    cfg_.sframe_dist = 4;
    cfg_.sframe_mode = 2;
    cfg_.sframe_type = 1;
  }

  void PreEncodeFrameHook(::libavm_test::VideoSource *video,
                          ::libavm_test::Encoder *encoder) override {
    if (video->frame() == 0) {
      encoder->Control(AVME_SET_CPUUSED, speed_);
      encoder->Control(AVME_SET_QP, 210);
      encoder->Control(AV2E_SET_ENABLE_KEYFRAME_FILTERING, 0);
      encoder->Control(AV2E_SET_MIN_GF_INTERVAL, 8);
      encoder->Control(AV2E_SET_MAX_GF_INTERVAL, 8);
      encoder->Control(AV2E_SET_GF_MIN_PYRAMID_HEIGHT, 3);
      encoder->Control(AV2E_SET_GF_MAX_PYRAMID_HEIGHT, 3);
      encoder->Control(AVME_SET_ENABLEAUTOALTREF, 1);
      encoder->Control(AV2E_SET_DELTAQ_MODE, 0);
      encoder->Control(AV2E_SET_ENABLE_TPL_MODEL, 0);
      encoder->Control(AV2E_SET_ENABLE_EXPLICIT_REF_FRAME_MAP, 1);
      encoder->SetOption("enable-intrabc-ext", "2");
    }
  }

  bool DoDecode() const override { return false; }

  bool HandleDecodeResult(const avm_codec_err_t res_dec,
                          libavm_test::Decoder *decoder) override {
    EXPECT_EQ(AVM_CODEC_OK, res_dec) << decoder->DecodeError();
    return AVM_CODEC_OK == res_dec;
  }

  void FramePktHook(const avm_codec_cx_pkt_t *pkt,
                    ::libavm_test::DxDataIterator *dec_iter) override {
    (void)dec_iter;
    if (pkt->kind != AVM_CODEC_CX_FRAME_PKT) return;
    const uint8_t *buf = static_cast<const uint8_t *>(pkt->data.frame.buf);
    const size_t sz = pkt->data.frame.sz;
    packets_.emplace_back(buf, buf + sz);
  }

  int speed_;
  std::vector<std::vector<uint8_t>> packets_;
};

TEST_P(RandomAccessRASPruningTest, PruneFirstGoP) {
  ::libavm_test::Y4mVideoSource video("park_joy_90p_8_420.y4m", 0, 17);
  ASSERT_NO_FATAL_FAILURE(RunLoop(&video));
  ASSERT_FALSE(packets_.empty()) << "No packets produced";

  // Decode all packets to get original frames in display order.
  std::vector<avm_image_t *> orig_frames = DecodePackets(packets_);

  // Prune: keep KEY, skip entire first GoP (including its leading RAS),
  // keep everything from the second GoP's RAS onward.
  std::vector<std::vector<uint8_t>> pruned = PruneFirstGoP(packets_);

  // Decode pruned packets.
  std::vector<avm_image_t *> pruned_frames = DecodePackets(pruned);

  // Verify the pruned bitstream decodes successfully and produces frames.
  ASSERT_FALSE(pruned_frames.empty()) << "Pruned decode produced no frames";

  // The original test used range (0,9): retained pictures 0 and 9-16 in
  // display order. With pyramid reordering, the decoder outputs frames in
  // display order. The pruned decode should produce the KEY frame plus
  // the second GoP frames. Verify the first pruned frame matches the
  // first original frame (both are KEY at POC 0).
  ASSERT_TRUE(ImagesMatch(orig_frames[0], pruned_frames[0]))
      << "KEY frame pixel mismatch";

  // Verify remaining pruned frames match the tail of the original stream.
  // The second GoP in display order occupies the last 8 positions of the
  // original 17-frame output (indices 9-16).
  const int second_gop_start = 9;
  const int second_gop_frames =
      static_cast<int>(orig_frames.size()) - second_gop_start;
  // pruned_frames[1..] should match orig_frames[9..16]
  for (int i = 0; i < second_gop_frames &&
                  (i + 1) < static_cast<int>(pruned_frames.size());
       ++i) {
    ASSERT_TRUE(
        ImagesMatch(orig_frames[second_gop_start + i], pruned_frames[1 + i]))
        << "Pixel mismatch at pruned frame " << (1 + i)
        << " (original display frame " << (second_gop_start + i) << ")";
  }

  FreeImages(orig_frames);
  FreeImages(pruned_frames);
}

AV2_INSTANTIATE_TEST_SUITE(RandomAccessRASPruningTest, ::testing::Values(9));

}  // namespace
