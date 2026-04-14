#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "av2/common/av2_common_int.h"
#include "av2/common/blockd.h"
#include "av2/common/entropy_inits_coeffs.h"
#include "av2/common/entropy_inits_modes.h"
#include "av2/common/entropy.h"
#include "av2/common/entropymode.h"
#include "av2/common/reconintra.h"
#include "av2/common/txb_common.h"
#include "av2/common/cfl.h"
#include "avm_dsp/bitwriter.h"
#include "av2/encoder/block.h"
#include "av2/encoder/encodetxb.h"

typedef struct FixtureWriter {
  avm_writer w;
  uint8_t *buffer;
  size_t capacity;
  MACROBLOCKD xd;
  PARTITION_CONTEXT above_partition_context_storage[MAX_MIB_SIZE * 4];
  ENTROPY_CONTEXT above_entropy_context_storage[MAX_MIB_SIZE * 4];
  MB_MODE_INFO shared_mbmi;
  MB_MODE_INFO *mi_slot;
  AV2_COMMON cm;
  FRAME_CONTEXT fc;
  FRAME_CONTEXT default_fc;
  avm_cdf_prob do_split_cdf[PARTITION_CONTEXTS][CDF_SIZE(2)];
  avm_cdf_prob do_square_split_cdf[SQUARE_SPLIT_CONTEXTS][CDF_SIZE(2)];
  avm_cdf_prob y_mode_set_cdf[CDF_SIZE(INTRA_MODE_SETS)];
  avm_cdf_prob y_mode_idx_cdf[Y_MODE_CONTEXTS][CDF_SIZE(LUMA_INTRA_MODE_INDEX_COUNT)];
  avm_cdf_prob uv_mode_cdf[UV_MODE_CONTEXTS][CDF_SIZE(CHROMA_INTRA_MODE_INDEX_COUNT)];
  avm_cdf_prob cfl_cdf[CFL_CONTEXTS][CDF_SIZE(2)];
  avm_cdf_prob filter_dir_cdf[MHCCP_CONTEXT_GROUP_SIZE][CDF_SIZE(MHCCP_MODE_NUM)];
  avm_cdf_prob txb_skip_cdf[TXB_SKIP_CONTEXTS][CDF_SIZE(2)];
  avm_cdf_prob v_txb_skip_cdf[V_TXB_SKIP_CONTEXTS][CDF_SIZE(2)];
} FixtureWriter;

static void die(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(1);
}

static void usage(const char *argv0) {
  fprintf(stderr,
          "Usage:\n"
          "  %s\n"
          "    Regenerate the checked-in oracle fixture from oracle_tg.bin.\n"
          "  %s --experimental-leaf <4|8|16|32|64>\n"
          "    Use the in-progress handcrafted generator path.\n",
          argv0, argv0);
}

static BLOCK_SIZE parse_leaf_bsize(const char *arg) {
  const int size = atoi(arg);
  switch (size) {
    case 4: return BLOCK_4X4;
    case 8: return BLOCK_8X8;
    case 16: return BLOCK_16X16;
    case 32: return BLOCK_32X32;
    case 64: return BLOCK_64X64;
    default: die("leaf size must be one of: 4 8 16 32 64");
  }
  return BLOCK_INVALID;
}

static void write_file(const char *path, const uint8_t *data, size_t len) {
  FILE *f = fopen(path, "wb");
  if (!f) {
    perror(path);
    exit(1);
  }
  if (fwrite(data, 1, len, f) != len) {
    perror(path);
    fclose(f);
    exit(1);
  }
  fclose(f);
}

static uint8_t *read_file(const char *path, size_t *len) {
  FILE *f = fopen(path, "rb");
  uint8_t *data;
  long file_len;

  if (!f) {
    perror(path);
    exit(1);
  }
  if (fseek(f, 0, SEEK_END) != 0) {
    perror(path);
    fclose(f);
    exit(1);
  }
  file_len = ftell(f);
  if (file_len < 0) {
    perror(path);
    fclose(f);
    exit(1);
  }
  if (fseek(f, 0, SEEK_SET) != 0) {
    perror(path);
    fclose(f);
    exit(1);
  }

  data = malloc((size_t)file_len);
  if (!data) die("alloc read buffer");
  if (fread(data, 1, (size_t)file_len, f) != (size_t)file_len) {
    perror(path);
    fclose(f);
    free(data);
    exit(1);
  }
  fclose(f);
  *len = (size_t)file_len;
  return data;
}

static void leb128_write(uint8_t *dst, size_t *offset, uint64_t value) {
  do {
    uint8_t byte = (uint8_t)(value & 0x7f);
    value >>= 7;
    if (value) byte |= 0x80;
    dst[(*offset)++] = byte;
  } while (value);
}

static void init_fixture_writer(FixtureWriter *fw) {
  memset(fw, 0, sizeof(*fw));
  fw->capacity = 1 << 20;
  fw->buffer = calloc(fw->capacity, 1);
  if (!fw->buffer) die("alloc fixture buffer");

  avm_start_encode(&fw->w, fw->buffer);
  fw->w.allow_update_cdf = 0;

  fw->cm.fc = &fw->fc;
  fw->cm.default_frame_context = &fw->default_fc;
  fw->cm.quant_params.base_qindex = 16;
  fw->cm.seq_params.enable_mhccp = 1;
  fw->cm.seq_params.enable_cfl_intra = 0;
  av2_set_default_frame_contexts(&fw->cm);

  fw->xd.tree_type = SHARED_PART;
  fw->xd.is_chroma_ref = 0;
  fw->mi_slot = &fw->shared_mbmi;
  fw->xd.mi = &fw->mi_slot;
  fw->xd.tile_ctx = &fw->fc;
  fw->xd.above_partition_context[0] = fw->above_partition_context_storage;
  fw->xd.above_partition_context[1] = fw->above_partition_context_storage + MAX_MIB_SIZE;
  fw->xd.left_partition_context[0][0] = 0;
  fw->xd.left_partition_context[1][0] = 0;
  fw->xd.above_entropy_context[0] = fw->above_entropy_context_storage;
  fw->xd.above_entropy_context[1] = fw->above_entropy_context_storage + MAX_MIB_SIZE;

  memset(fw->above_partition_context_storage, 0, sizeof(fw->above_partition_context_storage));
  memset(fw->above_entropy_context_storage, 0, sizeof(fw->above_entropy_context_storage));
  memset(fw->xd.left_partition_context, 0, sizeof(fw->xd.left_partition_context));
  memset(fw->xd.left_entropy_context, 0, sizeof(fw->xd.left_entropy_context));
  memset(&fw->shared_mbmi, 0, sizeof(fw->shared_mbmi));

  fw->shared_mbmi.mode = DC_PRED;
  fw->shared_mbmi.uv_mode = UV_DC_PRED;
  fw->shared_mbmi.y_mode_idx = 0;
  fw->shared_mbmi.uv_mode_idx = 0;
  fw->shared_mbmi.segment_id = 0;
  fw->shared_mbmi.skip_txfm[0] = 0;
  fw->shared_mbmi.skip_txfm[1] = 0;
  fw->shared_mbmi.fsc_mode[0] = 0;
  fw->shared_mbmi.fsc_mode[1] = 0;

  memcpy(fw->do_split_cdf, fw->fc.do_split_cdf[0], sizeof(fw->do_split_cdf));
  memcpy(fw->do_square_split_cdf, fw->fc.do_square_split_cdf[0],
         sizeof(fw->do_square_split_cdf));
  memcpy(fw->y_mode_set_cdf, fw->fc.y_mode_set_cdf, sizeof(fw->y_mode_set_cdf));
  memcpy(fw->y_mode_idx_cdf, fw->fc.y_mode_idx_cdf, sizeof(fw->y_mode_idx_cdf));
  memcpy(fw->uv_mode_cdf, fw->fc.uv_mode_cdf, sizeof(fw->uv_mode_cdf));
  memcpy(fw->cfl_cdf, fw->fc.cfl_cdf, sizeof(fw->cfl_cdf));
  memcpy(fw->filter_dir_cdf, fw->fc.filter_dir_cdf, sizeof(fw->filter_dir_cdf));
  memcpy(fw->txb_skip_cdf, fw->fc.txb_skip_cdf[0][TX_4X4],
         sizeof(fw->txb_skip_cdf));
  memcpy(fw->v_txb_skip_cdf, fw->fc.v_txb_skip_cdf,
         sizeof(fw->v_txb_skip_cdf));
}

static void write_partition_split(FixtureWriter *fw, int mi_row, int mi_col, BLOCK_SIZE bsize) {
  const int ctx = partition_plane_context(&fw->xd, mi_row, mi_col, bsize, 0, SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 1, fw->do_split_cdf[ctx], 2);
  const int square_ctx =
      partition_plane_context(&fw->xd, mi_row, mi_col, bsize, 0, SQUARE_SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 1, fw->do_square_split_cdf[square_ctx], 2);
}

static void write_partition_none(FixtureWriter *fw, int mi_row, int mi_col,
                                 BLOCK_SIZE bsize) {
  const int ctx =
      partition_plane_context(&fw->xd, mi_row, mi_col, bsize, 0,
                              SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 0, fw->do_split_cdf[ctx], 2);
}

static void write_partition_rect(FixtureWriter *fw, int mi_row, int mi_col,
                                 BLOCK_SIZE bsize, PARTITION_TYPE partition) {
  const int plane = 0;
  const int split_ctx =
      partition_plane_context(&fw->xd, mi_row, mi_col, bsize, 0,
                              SPLIT_CTX_MODE);
  const int square_split_ctx =
      partition_plane_context(&fw->xd, mi_row, mi_col, bsize, 0,
                              SQUARE_SPLIT_CTX_MODE);
  const RECT_PART_TYPE rect_type = get_rect_part_type(partition);
  const int rect_ctx =
      partition_plane_context(&fw->xd, mi_row, mi_col, bsize, 0,
                              RECT_TYPE_CTX_MODE);
  const int ext_ctx =
      partition_plane_context(&fw->xd, mi_row, mi_col, bsize, rect_type,
                              EXT_PART_CTX_MODE);

  avm_write_symbol(&fw->w, 1, fw->fc.do_split_cdf[plane][split_ctx], 2);
  avm_write_symbol(&fw->w, 0, fw->fc.do_square_split_cdf[plane][square_split_ctx], 2);
  avm_write_symbol(&fw->w, rect_type, fw->fc.rect_type_cdf[plane][rect_ctx],
                   NUM_RECT_PARTS);
  avm_write_symbol(&fw->w, 0, fw->fc.do_ext_partition_cdf[plane][0][ext_ctx], 2);
}

static void write_oracle_partition_sequence(FixtureWriter *fw) {
  const int plane = 0;
  int ctx;

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X128, 0, SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 0, fw->fc.do_split_cdf[plane][ctx], 2);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X128, 0,
                                SQUARE_SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 0, fw->fc.do_square_split_cdf[plane][ctx], 2);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X128, 0,
                                RECT_TYPE_CTX_MODE);
  avm_write_symbol(&fw->w, HORZ, fw->fc.rect_type_cdf[plane][ctx],
                   NUM_RECT_PARTS);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X128, HORZ,
                                EXT_PART_CTX_MODE);
  avm_write_symbol(&fw->w, 1, fw->fc.do_ext_partition_cdf[plane][0][ctx], 2);
  update_ext_partition_context(&fw->xd, 0, 0, BLOCK_128X64, BLOCK_128X128,
                               PARTITION_HORZ);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X64, 0, SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 0, fw->fc.do_split_cdf[plane][ctx], 2);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X64, 0,
                                SQUARE_SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 1, fw->fc.do_square_split_cdf[plane][ctx], 2);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X64, 0,
                                RECT_TYPE_CTX_MODE);
  avm_write_symbol(&fw->w, VERT, fw->fc.rect_type_cdf[plane][ctx],
                   NUM_RECT_PARTS);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_128X64, VERT,
                                EXT_PART_CTX_MODE);
  avm_write_symbol(&fw->w, 0, fw->fc.do_ext_partition_cdf[plane][0][ctx], 2);
  update_ext_partition_context(&fw->xd, 0, 0, BLOCK_64X64, BLOCK_128X64,
                               PARTITION_VERT);

  ctx = partition_plane_context(&fw->xd, 0, 0, BLOCK_64X64, 0, SPLIT_CTX_MODE);
  avm_write_symbol(&fw->w, 1, fw->fc.do_split_cdf[plane][ctx], 2);
}

static CHROMA_REF_INFO oracle_leaf_chroma_ref_info(void) {
  CHROMA_REF_INFO info;
  memset(&info, 0, sizeof(info));
  initialize_chroma_ref_info(0, 0, BLOCK_64X64, &info);
  return info;
}

static PARTITION_TYPE write_partition_native(
    const AV2_COMMON *cm, const MACROBLOCKD *xd, int mi_row, int mi_col,
    PARTITION_TYPE p, BLOCK_SIZE bsize, const PARTITION_TREE *ptree,
    const PARTITION_TREE *ptree_luma, avm_writer *w) {
  const int plane = xd->tree_type == CHROMA_PART;
  const int ssx = cm->seq_params.subsampling_x;
  const int ssy = cm->seq_params.subsampling_y;
  PARTITION_TYPE derived_partition = av2_get_normative_forced_partition_type(
      &cm->mi_params, xd->tree_type, ssx, ssy, mi_row, mi_col, bsize,
      ptree_luma);
  bool partition_allowed[ALL_PARTITION_TYPES];
  bool do_split = p != PARTITION_NONE;
  bool implied_do_split;
  bool do_ext_partition;
  bool implied_do_ext;
  RECT_PART_TYPE rect_type;
  FRAME_CONTEXT *ec_ctx;

  init_allowed_partitions_for_signaling(
      partition_allowed, cm, xd->tree_type,
      (ptree->parent ? ptree->parent->region_type : INTRA_REGION), mi_row,
      mi_col, ssx, ssy, bsize, &ptree->chroma_ref_info);

  if (derived_partition != PARTITION_INVALID &&
      partition_allowed[derived_partition]) {
    return derived_partition;
  }

  derived_partition = only_allowed_partition(partition_allowed);
  if (derived_partition != PARTITION_INVALID) {
    return derived_partition;
  }

  ec_ctx = xd->tile_ctx;
  if (is_do_split_implied(partition_allowed, &implied_do_split)) {
    if (!bru_is_sb_active(cm, mi_col, mi_row)) {
      do_split = false;
    }
  } else {
    if (!bru_is_sb_active(cm, mi_col, mi_row)) {
      do_split = false;
    } else {
      const int ctx =
          partition_plane_context(xd, mi_row, mi_col, bsize, 0, SPLIT_CTX_MODE);
      avm_write_symbol(w, do_split, ec_ctx->do_split_cdf[plane][ctx], 2);
    }
  }
  if (!do_split) return PARTITION_NONE;

  if (partition_allowed[PARTITION_SPLIT]) {
    const bool do_square_split = p == PARTITION_SPLIT;
    const int square_split_ctx = partition_plane_context(
        xd, mi_row, mi_col, bsize, 0, SQUARE_SPLIT_CTX_MODE);
    avm_write_symbol(w, do_square_split,
                     ec_ctx->do_square_split_cdf[plane][square_split_ctx], 2);
    if (do_square_split) return PARTITION_SPLIT;
  }

  rect_type = rect_type_implied_by_bsize(bsize, xd->tree_type);
  if (rect_type == RECT_INVALID) {
    rect_type = only_allowed_rect_type(partition_allowed);
  }
  if (rect_type == RECT_INVALID) {
    const int ctx = partition_plane_context(xd, mi_row, mi_col, bsize, 0,
                                            RECT_TYPE_CTX_MODE);
    rect_type = get_rect_part_type(p);
    avm_write_symbol(w, rect_type, ec_ctx->rect_type_cdf[plane][ctx],
                     NUM_RECT_PARTS);
  }

  do_ext_partition = (p >= PARTITION_HORZ_3);
  if (is_do_ext_partition_implied(partition_allowed, rect_type,
                                  &implied_do_ext)) {
    return p;
  }

  {
    const int ctx = partition_plane_context(xd, mi_row, mi_col, bsize,
                                            rect_type, EXT_PART_CTX_MODE);
    avm_write_symbol(w, do_ext_partition,
                     ec_ctx->do_ext_partition_cdf[plane][0][ctx], 2);
  }
  return p;
}

static void write_mh_dir_native(avm_cdf_prob *mh_dir_cdf, uint8_t mh_dir,
                                avm_writer *w) {
  avm_write_symbol(w, mh_dir, mh_dir_cdf, MHCCP_MODE_NUM);
}

static void write_intra_luma_mode_native(MACROBLOCKD *xd, avm_writer *w) {
  FRAME_CONTEXT *ec_ctx = xd->tile_ctx;
  MB_MODE_INFO *const mbmi = xd->mi[0];
  const int mode_idx = mbmi->y_mode_idx;
  const int context = get_y_mode_idx_ctx(xd);
  int mode_set_index = mode_idx < FIRST_MODE_COUNT ? 0 : 1;
  mode_set_index += ((mode_idx - FIRST_MODE_COUNT) / SECOND_MODE_COUNT);
  avm_write_symbol(w, mode_set_index, ec_ctx->y_mode_set_cdf, INTRA_MODE_SETS);
  if (mode_set_index == 0) {
    int mode_set_low = AVMMIN(mode_idx, LUMA_INTRA_MODE_INDEX_COUNT - 1);
    avm_write_symbol(w, mode_set_low, ec_ctx->y_mode_idx_cdf[context],
                     LUMA_INTRA_MODE_INDEX_COUNT);
    if (mode_set_low == (LUMA_INTRA_MODE_INDEX_COUNT - 1)) {
      avm_write_symbol(w, mode_idx - mode_set_low,
                       ec_ctx->y_mode_idx_offset_cdf[context],
                       LUMA_INTRA_MODE_OFFSET_COUNT);
    }
  } else {
    avm_write_literal(
        w,
        mode_idx - FIRST_MODE_COUNT - (mode_set_index - 1) * SECOND_MODE_COUNT,
        4);
  }
}

static void write_intra_uv_mode_native(MACROBLOCKD *xd,
                                       CFL_ALLOWED_TYPE cfl_allowed,
                                       avm_writer *w) {
  FRAME_CONTEXT *ec_ctx = xd->tile_ctx;
  MB_MODE_INFO *const mbmi = xd->mi[0];
  if (cfl_allowed) {
    const int cfl_ctx = get_cfl_ctx(xd);
    avm_write_symbol(w, mbmi->uv_mode == UV_CFL_PRED, ec_ctx->cfl_cdf[cfl_ctx],
                     2);
    if (mbmi->uv_mode == UV_CFL_PRED) return;
  }

  {
    const int uv_mode_idx = mbmi->uv_mode_idx;
    const int context = av2_is_directional_mode(mbmi->mode) ? 1 : 0;
    int mode_set_low =
        AVMMIN(uv_mode_idx, CHROMA_INTRA_MODE_INDEX_COUNT - 1);
    avm_write_symbol(w, mode_set_low, ec_ctx->uv_mode_cdf[context],
                     CHROMA_INTRA_MODE_INDEX_COUNT);
    if (mode_set_low == (CHROMA_INTRA_MODE_INDEX_COUNT - 1)) {
      avm_write_literal(w, uv_mode_idx - mode_set_low, 3);
    }
  }
}

static int write_sig_txtype_native(const AV2_COMMON *const cm,
                                   MACROBLOCK *const x, avm_writer *w,
                                   int blk_row, int blk_col, int plane,
                                   int block, TX_SIZE tx_size) {
  MACROBLOCKD *xd = &x->e_mbd;
  const CB_COEFF_BUFFER *cb_coef_buff = x->cb_coef_buff;
  const int txb_offset =
      x->mbmi_ext_frame->cb_offset[plane] / (TX_SIZE_W_MIN * TX_SIZE_H_MIN);
  const uint16_t *eob_txb = cb_coef_buff->eobs[plane] + txb_offset;
  const uint16_t eob = eob_txb[block];
  const uint8_t *entropy_ctx = cb_coef_buff->entropy_ctx[plane] + txb_offset;
  int txb_skip_ctx = (entropy_ctx[block] & TXB_SKIP_CTX_MASK);
  const TX_SIZE txs_ctx = get_txsize_entropy_ctx(tx_size);
  FRAME_CONTEXT *ec_ctx = xd->tile_ctx;
  const int is_inter = is_inter_block(xd->mi[0], xd->tree_type);

  (void)blk_row;
  (void)blk_col;

  if (plane == AVM_PLANE_V) {
    txb_skip_ctx += (xd->eob_u_flag ? V_TXB_SKIP_CONTEXT_OFFSET : 0);
  }
  if (plane == AVM_PLANE_U) {
    xd->eob_u_flag = eob ? 1 : 0;
  }

  if (plane == AVM_PLANE_Y || plane == AVM_PLANE_U) {
    const int pred_mode_ctx =
        (is_inter || xd->mi[0]->fsc_mode[xd->tree_type == CHROMA_PART]) ? 1 : 0;
    avm_write_symbol(w, eob == 0,
                     ec_ctx->txb_skip_cdf[pred_mode_ctx][txs_ctx][txb_skip_ctx],
                     2);
  } else {
    avm_write_symbol(w, eob == 0, ec_ctx->v_txb_skip_cdf[txb_skip_ctx], 2);
  }

  if (eob == 0) return 0;
  return 1;
}

static void write_oracle_native(FixtureWriter *fw) {
  MACROBLOCK *x;
  MB_MODE_INFO_EXT_FRAME *mbmi_ext_frame;
  CB_COEFF_BUFFER *cb_coef_buff;
  CHROMA_REF_INFO chroma_ref_info;
  MACROBLOCKD *xd;
  const int mi_row = 0;
  const int mi_col = 0;
  const BLOCK_SIZE root_bsize = BLOCK_128X128;
  const BLOCK_SIZE horz_subsize = get_partition_subsize(root_bsize, PARTITION_HORZ);
  const BLOCK_SIZE vert_subsize =
      get_partition_subsize(horz_subsize, PARTITION_VERT);

  x = calloc(1, sizeof(*x));
  mbmi_ext_frame = calloc(1, sizeof(*mbmi_ext_frame));
  cb_coef_buff = calloc(1, sizeof(*cb_coef_buff));
  if (!x || !mbmi_ext_frame || !cb_coef_buff) die("alloc native root state");
  xd = &x->e_mbd;
  memset(&chroma_ref_info, 0, sizeof(chroma_ref_info));

  fw->shared_mbmi.mode = DC_PRED;
  fw->shared_mbmi.y_mode_idx = 0;
  fw->shared_mbmi.uv_mode = UV_CFL_PRED;
  fw->shared_mbmi.uv_mode_idx = 0;
  fw->shared_mbmi.mh_dir = 0;
  fw->shared_mbmi.cfl_idx = CFL_MULTI_PARAM;
  fw->shared_mbmi.segment_id = 0;
  fw->shared_mbmi.skip_txfm[0] = 0;
  fw->shared_mbmi.skip_txfm[1] = 0;
  fw->shared_mbmi.fsc_mode[0] = 0;
  fw->shared_mbmi.fsc_mode[1] = 0;
  fw->shared_mbmi.sb_type[PLANE_TYPE_Y] = BLOCK_64X64;
  fw->shared_mbmi.sb_type[PLANE_TYPE_UV] = BLOCK_64X64;
  fw->shared_mbmi.tx_size = TX_64X64;
  fw->shared_mbmi.tx_partition_type[0] = TX_PARTITION_NONE;

  initialize_chroma_ref_info(0, 0, BLOCK_64X64, &chroma_ref_info);
  fw->shared_mbmi.chroma_ref_info = chroma_ref_info;

  xd->tree_type = SHARED_PART;
  xd->tile_ctx = &fw->fc;
  xd->mi = &fw->mi_slot;
  xd->mi_row = 0;
  xd->mi_col = 0;
  xd->is_chroma_ref = 1;
  xd->is_cfl_allowed_in_sdp = CFL_ALLOWED_FOR_CHROMA;
  xd->plane[AVM_PLANE_U].subsampling_x = 1;
  xd->plane[AVM_PLANE_U].subsampling_y = 1;
  xd->plane[AVM_PLANE_V].subsampling_x = 1;
  xd->plane[AVM_PLANE_V].subsampling_y = 1;
  xd->above_partition_context[0] = fw->above_partition_context_storage;
  xd->above_partition_context[1] = fw->above_partition_context_storage + MAX_MIB_SIZE;
  xd->above_entropy_context[0] = fw->above_entropy_context_storage;
  xd->above_entropy_context[1] = fw->above_entropy_context_storage + MAX_MIB_SIZE;

  x->cb_coef_buff = cb_coef_buff;
  x->mbmi_ext_frame = mbmi_ext_frame;

  write_oracle_partition_sequence(fw);
  write_intra_luma_mode_native(xd, &fw->w);
  write_intra_uv_mode_native(
      xd, is_cfl_allowed(fw->cm.seq_params.enable_cfl_intra, xd) ||
              is_mhccp_allowed(&fw->cm, xd),
      &fw->w);
  write_mh_dir_native(fw->fc.filter_dir_cdf[size_group_lookup[BLOCK_64X64]], 0,
                      &fw->w);
  write_sig_txtype_native(&fw->cm, x, &fw->w, 0, 0, AVM_PLANE_Y, 0,
                          TX_64X64);
  write_sig_txtype_native(&fw->cm, x, &fw->w, 0, 0, AVM_PLANE_U, 0,
                          TX_32X32);
  write_sig_txtype_native(&fw->cm, x, &fw->w, 0, 0, AVM_PLANE_V, 0,
                          TX_32X32);
  free(cb_coef_buff);
  free(mbmi_ext_frame);
  free(x);
}

static void write_luma_leaf(FixtureWriter *fw, BLOCK_SIZE bsize_base) {
  const ENTROPY_CONTEXT zeros[16] = { 0 };
  TXB_CTX txb_ctx;
  const TX_SIZE tx_size =
      bsize_base == BLOCK_4X4 ? TX_4X4 : get_sqr_tx_size(block_size_wide[bsize_base]);
  const int txs_ctx = get_txsize_entropy_ctx(tx_size);

  avm_write_symbol(&fw->w, 0, fw->y_mode_set_cdf, INTRA_MODE_SETS);
  avm_write_symbol(&fw->w, 0, fw->y_mode_idx_cdf[0], LUMA_INTRA_MODE_INDEX_COUNT);
  get_txb_ctx(bsize_base, tx_size, AVM_PLANE_Y, zeros, zeros, &txb_ctx, 0);
  avm_write_symbol(&fw->w, 1, fw->fc.txb_skip_cdf[0][txs_ctx][txb_ctx.txb_skip_ctx],
                   2);
}

static void write_chroma_leaf(FixtureWriter *fw, BLOCK_SIZE bsize_base) {
  const ENTROPY_CONTEXT zeros[16] = { 0 };
  TXB_CTX txb_ctx;
  const BLOCK_SIZE plane_bsize = get_plane_block_size(bsize_base, 1, 1);
  const TX_SIZE tx_size =
      bsize_base == BLOCK_4X4 ? TX_4X4 : av2_get_max_uv_txsize(bsize_base, 1, 1);
  const int txs_ctx = get_txsize_entropy_ctx(tx_size);

  if (bsize_base >= BLOCK_16X16) {
    avm_write_symbol(&fw->w, 1, fw->cfl_cdf[0], 2);
    avm_write_symbol(
        &fw->w, 0, fw->filter_dir_cdf[size_group_lookup[bsize_base]],
        MHCCP_MODE_NUM);
  } else {
    avm_write_symbol(&fw->w, 0, fw->uv_mode_cdf[0],
                     CHROMA_INTRA_MODE_INDEX_COUNT);
  }

  get_txb_ctx(plane_bsize, tx_size, AVM_PLANE_U, zeros, zeros, &txb_ctx, 0);
  avm_write_symbol(&fw->w, 1,
                   fw->fc.txb_skip_cdf[0][txs_ctx][txb_ctx.txb_skip_ctx], 2);

  get_txb_ctx(plane_bsize, tx_size, AVM_PLANE_V, zeros, zeros, &txb_ctx, 0);
  avm_write_symbol(&fw->w, 1, fw->fc.v_txb_skip_cdf[txb_ctx.txb_skip_ctx], 2);
}

static void write_partition_tree(FixtureWriter *fw, int mi_row, int mi_col,
                                 BLOCK_SIZE bsize, BLOCK_SIZE leaf_bsize,
                                 int index,
                                 const CHROMA_REF_INFO *parent_chroma_ref_info,
                                 BLOCK_SIZE parent_bsize,
                                 PARTITION_TYPE parent_partition,
                                 const PARTITION_TREE *parent_ptree) {
  CHROMA_REF_INFO chroma_ref_info;
  PARTITION_TREE ptree;
  PARTITION_TYPE partition = PARTITION_SPLIT;

  set_chroma_ref_info(SHARED_PART, mi_row, mi_col, index, bsize,
                      &chroma_ref_info, parent_chroma_ref_info, parent_bsize,
                      parent_partition, 1, 1);
  memset(&ptree, 0, sizeof(ptree));
  ptree.parent = (PARTITION_TREE *)parent_ptree;
  ptree.region_type = INTRA_REGION;
  ptree.bsize = bsize;
  ptree.mi_row = mi_row;
  ptree.mi_col = mi_col;
  ptree.index = index;
  ptree.chroma_ref_info = chroma_ref_info;
  fw->shared_mbmi.chroma_ref_info = chroma_ref_info;
  fw->xd.is_chroma_ref = chroma_ref_info.is_chroma_ref;
  if (bsize == leaf_bsize) {
    partition = PARTITION_NONE;
    if (bsize != BLOCK_4X4) {
      write_partition_native(&fw->cm, &fw->xd, mi_row, mi_col, partition, bsize,
                             &ptree, &ptree, &fw->w);
    }
    write_luma_leaf(fw, bsize);
    if (chroma_ref_info.is_chroma_ref) {
      const BLOCK_SIZE chroma_base =
          leaf_bsize == BLOCK_4X4 ? BLOCK_8X8 : leaf_bsize;
      write_chroma_leaf(fw, chroma_base);
    }
  } else {
    partition = write_partition_native(&fw->cm, &fw->xd, mi_row, mi_col,
                                       PARTITION_SPLIT, bsize, &ptree, &ptree,
                                       &fw->w);
    const BLOCK_SIZE subsize = get_partition_subsize(bsize, PARTITION_SPLIT);
    const int half_w = mi_size_wide[subsize];
    const int half_h = mi_size_high[subsize];

    write_partition_tree(fw, mi_row, mi_col, subsize, leaf_bsize, 0,
                         &chroma_ref_info, bsize, PARTITION_SPLIT, &ptree);
    write_partition_tree(fw, mi_row, mi_col + half_w, subsize, leaf_bsize, 1,
                         &chroma_ref_info, bsize, PARTITION_SPLIT, &ptree);
    write_partition_tree(fw, mi_row + half_h, mi_col, subsize, leaf_bsize, 2,
                         &chroma_ref_info, bsize, PARTITION_SPLIT, &ptree);
    write_partition_tree(fw, mi_row + half_h, mi_col + half_w, subsize,
                         leaf_bsize, 3, &chroma_ref_info, bsize,
                         PARTITION_SPLIT, &ptree);
  }

  const BLOCK_SIZE subsize = get_partition_subsize(bsize, partition);
  update_ext_partition_context(&fw->xd, mi_row, mi_col, subsize, bsize, partition);
}

static size_t finish_tile_payload(FixtureWriter *fw) {
  avm_stop_encode(&fw->w);
  return (size_t)fw->w.pos;
}

int main(int argc, char **argv) {
  const char *exec_name = argv[0];
  int experimental = 0;
  BLOCK_SIZE leaf_bsize = BLOCK_INVALID;
  const char *sh_path = "tests/corpora/m0/sh.bin";
  const char *fh_path = "tests/corpora/m0/fh.bin";
  const char *tg_path = "tests/corpora/m0/tg.bin";
  const char *oracle_tg_path = "oracle_tg.bin";
  const char *frame_obu_path = "tests/corpora/m0/frame_obu.bin";
  const char *ivf_path = "tests/corpora/m0/dc_intra_4x4.ivf";
  uint8_t *tg_data = NULL;
  size_t tg_len = 0;
  int own_tg_data = 0;

  if (argc == 1) {
    experimental = 0;
  } else if (argc == 3 && strcmp(argv[1], "--experimental-leaf") == 0) {
    experimental = 1;
    leaf_bsize = parse_leaf_bsize(argv[2]);
  } else {
    usage(exec_name);
    return 1;
  }

  FILE *f = fopen(sh_path, "rb");
  if (!f) die("missing sh.bin");
  fseek(f, 0, SEEK_END);
  long sh_len = ftell(f);
  fseek(f, 0, SEEK_SET);
  uint8_t *sh = malloc((size_t)sh_len);
  fread(sh, 1, (size_t)sh_len, f);
  fclose(f);

  f = fopen(fh_path, "rb");
  if (!f) die("missing fh.bin");
  fseek(f, 0, SEEK_END);
  long fh_len = ftell(f);
  fseek(f, 0, SEEK_SET);
  uint8_t *fh = malloc((size_t)fh_len);
  fread(fh, 1, (size_t)fh_len, f);
  fclose(f);

  if (!experimental) {
    tg_data = read_file(oracle_tg_path, &tg_len);
    own_tg_data = 1;
  } else {
    FixtureWriter *fw = calloc(1, sizeof(*fw));
    if (!fw) die("alloc fixture writer");
    init_fixture_writer(fw);
    if (leaf_bsize == BLOCK_64X64) {
      write_oracle_native(fw);
    } else {
      const CHROMA_REF_INFO root_chroma_ref_info = oracle_leaf_chroma_ref_info();

      fw->shared_mbmi.sb_type[PLANE_TYPE_Y] = BLOCK_64X64;
      fw->shared_mbmi.sb_type[PLANE_TYPE_UV] = BLOCK_64X64;
      fw->shared_mbmi.chroma_ref_info = root_chroma_ref_info;
      fw->xd.is_chroma_ref = root_chroma_ref_info.is_chroma_ref;

      write_oracle_partition_sequence(fw);
      write_partition_tree(fw, 0, 0, BLOCK_64X64, leaf_bsize, 0,
                           &root_chroma_ref_info, BLOCK_128X64,
                           PARTITION_VERT, NULL);
    }
    tg_len = finish_tile_payload(fw);
    tg_data = malloc(tg_len);
    if (!tg_data) die("alloc tile-group output");
    memcpy(tg_data, fw->buffer, tg_len);
    own_tg_data = 1;
    free(fw->buffer);
    free(fw);
  }

  write_file(tg_path, tg_data, tg_len);

  size_t frame_obu_len = 1 + (size_t)fh_len + tg_len;
  uint8_t *frame_obu = malloc(frame_obu_len);
  frame_obu[0] = 0x10;
  memcpy(frame_obu + 1, fh, (size_t)fh_len);
  memcpy(frame_obu + 1 + fh_len, tg_data, tg_len);
  write_file(frame_obu_path, frame_obu, frame_obu_len);

  size_t frame_unit_cap = 64 + (size_t)sh_len + frame_obu_len;
  uint8_t *frame_unit = calloc(frame_unit_cap, 1);
  size_t off = 0;
  leb128_write(frame_unit, &off, 1);
  frame_unit[off++] = 0x08;
  leb128_write(frame_unit, &off, 1 + (size_t)sh_len);
  frame_unit[off++] = 0x04;
  memcpy(frame_unit + off, sh, (size_t)sh_len);
  off += (size_t)sh_len;
  leb128_write(frame_unit, &off, frame_obu_len);
  memcpy(frame_unit + off, frame_obu, frame_obu_len);
  off += frame_obu_len;

  uint8_t ivf_header[32] = {
      'D', 'K', 'I', 'F', 0, 0, 32, 0, 'A', 'V', '0', '2',
      64, 0, 64, 0, 1, 0, 0, 0, 1, 0, 0, 0,
      1, 0, 0, 0, 0, 0, 0, 0,
  };
  uint8_t frame_header[12] = { 0 };
  frame_header[0] = (uint8_t)(off & 0xff);
  frame_header[1] = (uint8_t)((off >> 8) & 0xff);
  frame_header[2] = (uint8_t)((off >> 16) & 0xff);
  frame_header[3] = (uint8_t)((off >> 24) & 0xff);

  FILE *ivf = fopen(ivf_path, "wb");
  if (!ivf) {
    perror(ivf_path);
    return 1;
  }
  fprintf(stderr, "ivf fourcc bytes: %c%c%c%c\n", ivf_header[8], ivf_header[9],
          ivf_header[10], ivf_header[11]);
  fwrite(ivf_header, 1, sizeof(ivf_header), ivf);
  fwrite(frame_header, 1, sizeof(frame_header), ivf);
  fwrite(frame_unit, 1, off, ivf);
  fclose(ivf);

  if (experimental) {
    fprintf(stderr,
            "wrote %s (%zu bytes tile payload, experimental handcrafted leaf=%d)\n",
            ivf_path, tg_len, block_size_wide[leaf_bsize]);
  } else {
    fprintf(stderr,
            "wrote %s (%zu bytes tile payload, checked-in oracle fixture)\n",
            ivf_path, tg_len);
  }

  free(sh);
  free(fh);
  free(frame_obu);
  free(frame_unit);
  if (own_tg_data) free(tg_data);
  return 0;
}
