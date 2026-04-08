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

// Test for restricted_prediction_switch output ordering.
//
// Verifies that when a switch frame with restricted_prediction_switch=1
// is decoded, eligible frames in ref_frame_map are output in ascending
// display_order_hint order.
//
// Encodes with RA pyramid coding (which stores frames in non-display
// order in ref_frame_map) and uses AV2E_SET_FORCE_DEFERRED_FRAMES_FOR_RAS_TEST
// to defer output of inter frames so they accumulate in ref_frame_map.
// When the switch frame is decoded, the restricted_prediction_switch
// path outputs these deferred frames.
//
// The decoder's av2_output_frame_buffers() conformance check
// (check_and_update_output_doh) will cause a decode error if frames
// are output out of order, so the test simply verifies that decoding
// succeeds.

#include <vector>

#include "third_party/googletest/src/googletest/include/gtest/gtest.h"
#include "test/codec_factory.h"
#include "test/encode_test_driver.h"
#include "test/y4m_video_source.h"
#include "test/util.h"
#include "av2/common/enums.h"

namespace {

class OutputOrderTest : public ::libavm_test::CodecTestWithParam<int>,
                        public ::libavm_test::EncoderTest {
 protected:
  OutputOrderTest() : EncoderTest(GET_PARAM(0)), speed_(GET_PARAM(1)) {}

  void SetUp() override {
    InitializeConfig();
    passes_ = 1;
    cfg_.rc_end_usage = AVM_Q;
    cfg_.g_threads = 1;
    cfg_.g_lag_in_frames = 9;
    cfg_.kf_min_dist = 65;
    cfg_.kf_max_dist = 65;
    cfg_.use_fixed_qp_offsets = 1;
    cfg_.sframe_dist = 4;
    cfg_.sframe_mode = 0;
    cfg_.sframe_type = 1;
  }

  void PreEncodeFrameHook(::libavm_test::VideoSource *video,
                          ::libavm_test::Encoder *encoder) override {
    if (video->frame() == 0) {
      encoder->Control(AVME_SET_CPUUSED, speed_);
      encoder->Control(AVME_SET_QP, 210);
      encoder->Control(AV2E_SET_ENABLE_KEYFRAME_FILTERING, 0);
      encoder->Control(AV2E_SET_MIN_GF_INTERVAL, 4);
      encoder->Control(AV2E_SET_MAX_GF_INTERVAL, 4);
      encoder->Control(AV2E_SET_GF_MIN_PYRAMID_HEIGHT, 2);
      encoder->Control(AV2E_SET_GF_MAX_PYRAMID_HEIGHT, 2);
      encoder->Control(AVME_SET_ENABLEAUTOALTREF, 1);
      encoder->Control(AV2E_SET_DELTAQ_MODE, 0);
      encoder->Control(AV2E_SET_ENABLE_TPL_MODEL, 0);
      encoder->Control(AV2E_SET_ENABLE_EXPLICIT_REF_FRAME_MAP, 1);
      encoder->SetOption("enable-intrabc-ext", "2");
      encoder->Control(AV2E_SET_FORCE_DEFERRED_FRAMES_FOR_RAS_TEST, 1);
    }
  }

  bool DoDecode() const override { return false; }

  void FramePktHook(const avm_codec_cx_pkt_t *pkt,
                    ::libavm_test::DxDataIterator *dec_iter) override {
    (void)dec_iter;
    if (pkt->kind != AVM_CODEC_CX_FRAME_PKT) return;
    const uint8_t *buf = static_cast<const uint8_t *>(pkt->data.frame.buf);
    packets_.emplace_back(buf, buf + pkt->data.frame.sz);
  }

  int speed_;
  std::vector<std::vector<uint8_t>> packets_;
};

TEST_P(OutputOrderTest, VerifyOutputOrderAtRAS) {
  ::libavm_test::Y4mVideoSource video("park_joy_90p_8_420.y4m", 0, 9);
  ASSERT_NO_FATAL_FAILURE(RunLoop(&video));
  ASSERT_FALSE(packets_.empty());

  // Decode all packets. The decoder's conformance check in
  // av2_output_frame_buffers() will trigger a fatal error if output
  // frames are not in ascending display_order_hint order.
  avm_codec_dec_cfg_t c = {};
  c.threads = 1;
  libavm_test::AV2Decoder dec(c);

  for (const auto &p : packets_) {
    ASSERT_EQ(dec.DecodeFrame(p.data(), p.size()), AVM_CODEC_OK);
    libavm_test::DxDataIterator it = dec.GetDxData();
    while (it.Next()) {
    }
  }
  // Flush the decoder.
  ASSERT_EQ(dec.DecodeFrame(NULL, 0), AVM_CODEC_OK);
  {
    libavm_test::DxDataIterator it = dec.GetDxData();
    while (it.Next()) {
    }
  }
}

AV2_INSTANTIATE_TEST_SUITE(OutputOrderTest, ::testing::Values(9));

}  // namespace
