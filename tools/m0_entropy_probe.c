#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "av2/common/av2_common_int.h"
#include "av2/common/blockd.h"
#include "av2/common/cfl.h"
#include "av2/common/entropy.h"
#include "av2/common/reconintra.h"
#include "av2/common/txb_common.h"
#include "avm_dsp/bitreader.h"
#include "config/av2_rtcd.h"
#include "config/avm_dsp_rtcd.h"

typedef struct ProbeState {
  AV2_COMMON cm;
  FRAME_CONTEXT fc;
  FRAME_CONTEXT default_fc;
  MACROBLOCKD xd;
  MB_MODE_INFO shared_mbmi;
  MB_MODE_INFO *mi_slot;
  PARTITION_TREE root_ptree;
  PARTITION_TREE child_ptree;
  PARTITION_TREE leaf_ptree;
  PARTITION_CONTEXT above_partition_context_storage[MAX_MIB_SIZE * 4];
  ENTROPY_CONTEXT above_entropy_context_storage[MAX_MIB_SIZE * 4];
} ProbeState;

static void die(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(EXIT_FAILURE);
}

static uint8_t *read_file(const char *path, size_t *len) {
  FILE *f = fopen(path, "rb");
  uint8_t *data;
  long file_len;

  if (!f) {
    perror(path);
    exit(EXIT_FAILURE);
  }

  if (fseek(f, 0, SEEK_END) != 0) {
    perror(path);
    fclose(f);
    exit(EXIT_FAILURE);
  }
  file_len = ftell(f);
  if (file_len < 0) {
    perror(path);
    fclose(f);
    exit(EXIT_FAILURE);
  }
  if (fseek(f, 0, SEEK_SET) != 0) {
    perror(path);
    fclose(f);
    exit(EXIT_FAILURE);
  }

  data = malloc((size_t)file_len);
  if (!data) die("alloc tg buffer");
  if (fread(data, 1, (size_t)file_len, f) != (size_t)file_len) {
    perror(path);
    fclose(f);
    free(data);
    exit(EXIT_FAILURE);
  }

  fclose(f);
  *len = (size_t)file_len;
  return data;
}

static void init_probe_state(ProbeState *state) {
  bool partition_allowed[ALL_PARTITION_TYPES];
  memset(state, 0, sizeof(*state));

  state->cm.fc = &state->fc;
  state->cm.default_frame_context = &state->default_fc;
  state->cm.quant_params.base_qindex = 16;
  state->cm.mi_params.mi_rows = 16;
  state->cm.mi_params.mi_cols = 16;
  state->cm.mi_params.mi_stride = 16;
  state->cm.sb_size = BLOCK_128X128;
  state->cm.seq_params.sb_size = BLOCK_128X128;
  state->cm.seq_params.subsampling_x = 1;
  state->cm.seq_params.subsampling_y = 1;
  state->cm.seq_params.enable_mhccp = 1;
  state->cm.seq_params.enable_cfl_intra = 0;
  state->cm.seq_params.enable_ext_partitions = 1;
  state->cm.seq_params.enable_uneven_4way_partitions = 0;
  state->cm.seq_params.max_pb_aspect_ratio_log2_m1 = 0;
  av2_set_default_frame_contexts(&state->cm);

  state->mi_slot = &state->shared_mbmi;
  state->xd.mi = &state->mi_slot;
  state->xd.tile_ctx = &state->fc;
  state->xd.tree_type = SHARED_PART;
  state->xd.is_cfl_allowed_in_sdp = CFL_ALLOWED_FOR_CHROMA;
  state->xd.above_partition_context[0] =
      state->above_partition_context_storage;
  state->xd.above_partition_context[1] =
      state->above_partition_context_storage + MAX_MIB_SIZE;
  state->xd.above_entropy_context[0] = state->above_entropy_context_storage;
  state->xd.above_entropy_context[1] =
      state->above_entropy_context_storage + MAX_MIB_SIZE;

  state->shared_mbmi.sb_type[PLANE_TYPE_Y] = BLOCK_64X64;
  state->shared_mbmi.sb_type[PLANE_TYPE_UV] = BLOCK_64X64;
  state->shared_mbmi.chroma_ref_info.bsize_base = BLOCK_64X64;
  state->shared_mbmi.mode = DC_PRED;
  state->shared_mbmi.uv_mode = UV_CFL_PRED;
  state->shared_mbmi.y_mode_idx = 0;
  state->shared_mbmi.uv_mode_idx = 0;
  state->shared_mbmi.mh_dir = 0;
  state->shared_mbmi.segment_id = 0;
  state->shared_mbmi.tx_size = TX_64X64;
  state->shared_mbmi.chroma_ref_info.is_chroma_ref = 1;
  state->root_ptree.region_type = INTRA_REGION;
  state->root_ptree.bsize = BLOCK_128X128;
  state->root_ptree.mi_row = 0;
  state->root_ptree.mi_col = 0;
  state->root_ptree.index = 0;
  set_chroma_ref_info(SHARED_PART, 0, 0, 0, BLOCK_128X128,
                      &state->root_ptree.chroma_ref_info, NULL, BLOCK_INVALID,
                      PARTITION_NONE, 1, 1);
  state->child_ptree.parent = &state->root_ptree;
  state->child_ptree.region_type = INTRA_REGION;
  state->child_ptree.bsize = BLOCK_128X64;
  state->child_ptree.mi_row = 0;
  state->child_ptree.mi_col = 0;
  state->child_ptree.index = 0;
  state->root_ptree.partition = PARTITION_HORZ;
  set_chroma_ref_info(SHARED_PART, 0, 0, 0, BLOCK_128X64,
                      &state->child_ptree.chroma_ref_info,
                      &state->root_ptree.chroma_ref_info, BLOCK_128X128,
                      PARTITION_HORZ, 1, 1);
  state->leaf_ptree.parent = &state->child_ptree;
  state->leaf_ptree.region_type = INTRA_REGION;
  state->leaf_ptree.bsize = BLOCK_64X64;
  state->leaf_ptree.mi_row = 0;
  state->leaf_ptree.mi_col = 0;
  state->leaf_ptree.index = 0;
  state->child_ptree.partition = PARTITION_VERT;
  set_chroma_ref_info(SHARED_PART, 0, 0, 0, BLOCK_64X64,
                      &state->leaf_ptree.chroma_ref_info,
                      &state->child_ptree.chroma_ref_info, BLOCK_128X64,
                      PARTITION_VERT, 1, 1);

  init_allowed_partitions_for_signaling(
      partition_allowed, &state->cm, SHARED_PART, INTRA_REGION, 0, 0, 1, 1,
      BLOCK_128X128, &state->root_ptree.chroma_ref_info);

  state->xd.plane[AVM_PLANE_U].subsampling_x = 1;
  state->xd.plane[AVM_PLANE_U].subsampling_y = 1;
  state->xd.plane[AVM_PLANE_V].subsampling_x = 1;
  state->xd.plane[AVM_PLANE_V].subsampling_y = 1;
}

static void print_reader_state(const char *label, avm_reader *reader) {
  printf("%s: tell=%u tell_frac=%llu overflow=%d\n", label,
         avm_reader_tell(reader), (unsigned long long)avm_reader_tell_frac(reader),
         avm_reader_has_overflowed(reader));
}

int main(int argc, char **argv) {
  size_t data_len = 0;
  uint8_t *data = NULL;
  ProbeState *state = NULL;
  avm_reader reader;
  const ENTROPY_CONTEXT zeros[16] = { 0 };
  TXB_CTX txb_ctx;
  int split_ctx;
  int square_split_ctx;
  int rect_ctx;
  int ext_ctx;
  int y_mode_ctx;
  int cfl_ctx;
  int pred_mode_ctx;
  int txs_ctx;
  int txb_skip_ctx;
  int symbol;

  if (argc != 2) {
    fprintf(stderr, "Usage: %s <tg.bin>\n", argv[0]);
    return EXIT_FAILURE;
  }

  setvbuf(stdout, NULL, _IONBF, 0);
  av2_rtcd();
  avm_dsp_rtcd();
  data = read_file(argv[1], &data_len);
  printf("loaded %zu bytes from %s\n", data_len, argv[1]);
  state = calloc(1, sizeof(*state));
  if (!state) {
    free(data);
    die("alloc probe state");
  }
  init_probe_state(state);
  printf("probe state initialized\n");

  memset(&reader, 0, sizeof(reader));
  if (avm_reader_init(&reader, data, data_len) != 0) {
    free(state);
    free(data);
    die("avm_reader_init failed");
  }
  reader.allow_update_cdf = 1;
  printf("reader initialized\n");

  printf("reading root do_split\n");
  split_ctx =
      partition_plane_context(&state->xd, 0, 0, BLOCK_128X128, 0, SPLIT_CTX_MODE);
  printf("root split_ctx=%d\n", split_ctx);
  symbol = avm_read_symbol(&reader, state->fc.do_split_cdf[0][split_ctx], 2,
                           ACCT_INFO("probe_root_do_split"));
  printf("root do_split=%d ctx=%d\n", symbol, split_ctx);
  print_reader_state("after root do_split", &reader);

  printf("reading root do_square_split\n");
  square_split_ctx =
      partition_plane_context(&state->xd, 0, 0, BLOCK_128X128, 0,
                              SQUARE_SPLIT_CTX_MODE);
  symbol = avm_read_symbol(&reader, state->fc.do_square_split_cdf[0][square_split_ctx],
                           2, ACCT_INFO("probe_root_do_square_split"));
  printf("root do_square_split=%d ctx=%d\n", symbol, square_split_ctx);
  print_reader_state("after root do_square_split", &reader);

  printf("reading root rect_type\n");
  rect_ctx =
      partition_plane_context(&state->xd, 0, 0, BLOCK_128X128, 0,
                              RECT_TYPE_CTX_MODE);
  symbol = avm_read_symbol(&reader, state->fc.rect_type_cdf[0][rect_ctx],
                           NUM_RECT_PARTS, ACCT_INFO("probe_root_rect_type"));
  printf("root rect_type=%d ctx=%d\n", symbol, rect_ctx);
  print_reader_state("after root rect_type", &reader);

  printf("reading root do_ext_partition\n");
  ext_ctx = partition_plane_context(&state->xd, 0, 0, BLOCK_128X128, symbol,
                                    EXT_PART_CTX_MODE);
  symbol = avm_read_symbol(&reader,
                           state->fc.do_ext_partition_cdf[0][0][ext_ctx], 2,
                           ACCT_INFO("probe_root_do_ext_partition"));
  printf("root do_ext_partition=%d ctx=%d\n", symbol, ext_ctx);
  print_reader_state("after root do_ext_partition", &reader);

  update_ext_partition_context(&state->xd, 0, 0, BLOCK_128X64, BLOCK_128X128,
                               PARTITION_HORZ);

  printf("reading child do_split\n");
  split_ctx =
      partition_plane_context(&state->xd, 0, 0, BLOCK_128X64, 0, SPLIT_CTX_MODE);
  symbol = avm_read_symbol(&reader, state->fc.do_split_cdf[0][split_ctx], 2,
                           ACCT_INFO("probe_child_do_split"));
  printf("child do_split=%d ctx=%d\n", symbol, split_ctx);
  print_reader_state("after child do_split", &reader);

  printf("reading child do_square_split\n");
  square_split_ctx =
      partition_plane_context(&state->xd, 0, 0, BLOCK_128X64, 0,
                              SQUARE_SPLIT_CTX_MODE);
  symbol = avm_read_symbol(&reader, state->fc.do_square_split_cdf[0][square_split_ctx],
                           2, ACCT_INFO("probe_child_do_square_split"));
  printf("child do_square_split=%d ctx=%d\n", symbol, square_split_ctx);
  print_reader_state("after child do_square_split", &reader);

  printf("reading child rect_type\n");
  rect_ctx =
      partition_plane_context(&state->xd, 0, 0, BLOCK_128X64, 0,
                              RECT_TYPE_CTX_MODE);
  symbol = avm_read_symbol(&reader, state->fc.rect_type_cdf[0][rect_ctx],
                           NUM_RECT_PARTS, ACCT_INFO("probe_child_rect_type"));
  printf("child rect_type=%d ctx=%d\n", symbol, rect_ctx);
  print_reader_state("after child rect_type", &reader);

  printf("reading child do_ext_partition\n");
  ext_ctx = partition_plane_context(&state->xd, 0, 0, BLOCK_128X64, symbol,
                                    EXT_PART_CTX_MODE);
  symbol = avm_read_symbol(&reader,
                           state->fc.do_ext_partition_cdf[0][0][ext_ctx], 2,
                           ACCT_INFO("probe_child_do_ext_partition"));
  printf("child do_ext_partition=%d ctx=%d\n", symbol, ext_ctx);
  print_reader_state("after child do_ext_partition", &reader);

  update_ext_partition_context(&state->xd, 0, 0, BLOCK_64X64, BLOCK_128X64,
                               PARTITION_VERT);

  printf("reading leaf do_split\n");
  split_ctx =
      partition_plane_context(&state->xd, 0, 0, BLOCK_64X64, 0, SPLIT_CTX_MODE);
  symbol = avm_read_symbol(&reader, state->fc.do_split_cdf[0][split_ctx], 2,
                           ACCT_INFO("probe_leaf_do_split"));
  printf("leaf do_split=%d ctx=%d\n", symbol, split_ctx);
  print_reader_state("after leaf do_split", &reader);

  printf("reading y_mode_set\n");
  y_mode_ctx = get_y_mode_idx_ctx(&state->xd);
  symbol = avm_read_symbol(&reader, state->fc.y_mode_set_cdf, INTRA_MODE_SETS,
                           ACCT_INFO("probe_mode_set"));
  printf("mode_set_index=%d ctx=%d\n", symbol, y_mode_ctx);
  print_reader_state("after y_mode_set", &reader);

  printf("reading y_mode_idx\n");
  symbol = avm_read_symbol(&reader, state->fc.y_mode_idx_cdf[y_mode_ctx],
                           LUMA_INTRA_MODE_INDEX_COUNT,
                           ACCT_INFO("probe_mode_idx"));
  printf("mode_idx=%d ctx=%d\n", symbol, y_mode_ctx);
  print_reader_state("after y_mode_idx", &reader);

  printf("reading is_cfl\n");
  cfl_ctx = get_cfl_ctx(&state->xd);
  symbol = avm_read_symbol(&reader, state->fc.cfl_cdf[cfl_ctx], 2,
                           ACCT_INFO("probe_is_cfl"));
  printf("is_cfl_idx=%d ctx=%d\n", symbol, cfl_ctx);
  print_reader_state("after is_cfl", &reader);

  printf("reading mh_dir\n");
  symbol = avm_read_symbol(&reader,
                           state->fc.filter_dir_cdf[size_group_lookup[BLOCK_64X64]],
                           MHCCP_MODE_NUM, ACCT_INFO("probe_mh_dir"));
  printf("mh_dir=%d size_group=%d\n", symbol, size_group_lookup[BLOCK_64X64]);
  print_reader_state("after mh_dir", &reader);

  printf("reading all_zero_y\n");
  get_txb_ctx(BLOCK_64X64, TX_64X64, AVM_PLANE_Y, zeros, zeros, &txb_ctx, 0);
  pred_mode_ctx = 0;
  txs_ctx = get_txsize_entropy_ctx(TX_64X64);
  txb_skip_ctx = txb_ctx.txb_skip_ctx;
  symbol = avm_read_symbol(&reader,
                           state->fc.txb_skip_cdf[pred_mode_ctx][txs_ctx][txb_skip_ctx],
                           2, ACCT_INFO("probe_all_zero_y"));
  printf("all_zero_y=%d ctx=%d txs_ctx=%d\n", symbol, txb_skip_ctx, txs_ctx);
  print_reader_state("after all_zero_y", &reader);

  printf("reading all_zero_u\n");
  get_txb_ctx(BLOCK_64X64, TX_32X32, AVM_PLANE_U, zeros, zeros, &txb_ctx, 0);
  txb_skip_ctx = txb_ctx.txb_skip_ctx;
  symbol = avm_read_symbol(&reader,
                           state->fc.txb_skip_cdf[pred_mode_ctx][txs_ctx][txb_skip_ctx],
                           2, ACCT_INFO("probe_all_zero_u"));
  printf("all_zero_u=%d ctx=%d txs_ctx=%d\n", symbol, txb_skip_ctx, txs_ctx);
  print_reader_state("after all_zero_u", &reader);

  printf("reading all_zero_v\n");
  get_txb_ctx(BLOCK_64X64, TX_32X32, AVM_PLANE_V, zeros, zeros, &txb_ctx, 0);
  txb_skip_ctx = txb_ctx.txb_skip_ctx;
  symbol = avm_read_symbol(&reader, state->fc.v_txb_skip_cdf[txb_skip_ctx], 2,
                           ACCT_INFO("probe_all_zero_v"));
  printf("all_zero_v=%d ctx=%d\n", symbol, txb_skip_ctx);
  print_reader_state("after all_zero_v", &reader);

  free(state);
  free(data);
  return EXIT_SUCCESS;
}
