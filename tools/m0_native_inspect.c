/*
 * Copyright (c) 2026, Alliance for Open Media. All rights reserved
 *
 * This source code is subject to the terms of the BSD 3-Clause Clear License
 * and the Alliance for Open Media Patent License 1.0.
 */

#include <stdio.h>
#include <stdlib.h>

#include "avm/avm_decoder.h"
#include "avm/avmdx.h"
#include "av2/decoder/inspection.h"
#include "common/tools_common.h"
#include "common/video_reader.h"

static const char *g_exec_name = "m0_native_inspect";

typedef struct InspectCtx {
  insp_frame_data frame_data;
  int frame_ready;
  int sb_ready;
} InspectCtx;

void usage_exit(void) {
  fprintf(stderr, "Usage: %s <input.ivf>\n", g_exec_name);
  exit(EXIT_FAILURE);
}

static void print_tree(const PARTITION_TREE *node, int depth) {
  int i;
  if (!node) return;
  for (i = 0; i < depth; ++i) printf("  ");
  printf(
      "node depth=%d idx=%d part=%d bsize=%d mi=(%d,%d) settled=%d chroma_base=%d\n",
      depth, node->index, node->partition, node->bsize, node->mi_row,
      node->mi_col, node->is_settled, node->chroma_ref_info.bsize_base);
  for (i = 0; i < 4; ++i) {
    if (node->sub_tree[i] != NULL) print_tree(node->sub_tree[i], depth + 1);
  }
}

static void inspect_frame_cb(void *decoder, void *data) {
  InspectCtx *ctx = (InspectCtx *)data;
  ifd_inspect(&ctx->frame_data, decoder, 0);
  ctx->frame_ready = 1;
}

static void inspect_sb_cb(void *decoder, void *data) {
  InspectCtx *ctx = (InspectCtx *)data;
  ifd_inspect_superblock(&ctx->frame_data, decoder);
  ctx->sb_ready = 1;
}

int main(int argc, char **argv) {
  AvxVideoReader *reader;
  const AvxVideoInfo *video_info;
  avm_codec_iface_t *decoder;
  avm_codec_ctx_t codec;
  avm_inspect_init inspect_init;
  InspectCtx inspect_ctx;
  size_t frame_size = 0;
  const unsigned char *frame;
  const insp_mi_data *mi00;
  const insp_sb_data *sb00;

  g_exec_name = argv[0];
  if (argc != 2) usage_exit();

  reader = avm_video_reader_open(argv[1]);
  if (!reader) die("Failed to open %s", argv[1]);
  video_info = avm_video_reader_get_info(reader);
  (void)video_info;
  if (get_avm_decoder_count() < 1) die("No decoder interfaces are available.");
  decoder = get_avm_decoder_by_index(0);

  if (avm_codec_dec_init(&codec, decoder, NULL, 0))
    die("Failed to initialize decoder.");

  inspect_ctx.frame_ready = 0;
  inspect_ctx.sb_ready = 0;
  ifd_init(&inspect_ctx.frame_data, video_info->frame_width, video_info->frame_height);

  inspect_init.inspect_cb = inspect_frame_cb;
  inspect_init.inspect_sb_cb = inspect_sb_cb;
  inspect_init.inspect_tip_cb = NULL;
  inspect_init.inspect_ctx = &inspect_ctx;
  if (avm_codec_control(&codec, AV2_SET_INSPECTION_CALLBACK, &inspect_init))
    die_codec(&codec, "Failed to set inspection callback.");

  if (!avm_video_reader_read_frame(reader)) die("Failed to read first frame.");
  frame = avm_video_reader_get_frame(reader, &frame_size);
  if (avm_codec_decode(&codec, frame, frame_size, NULL))
    die_codec(&codec, "Failed to decode frame.");

  if (!inspect_ctx.frame_ready) die("Frame inspection callback did not run.");
  if (!inspect_ctx.sb_ready) die("Superblock inspection callback did not run.");

  mi00 = &inspect_ctx.frame_data.mi_grid[0];
  sb00 = &inspect_ctx.frame_data.sb_grid[0];

  printf("frame_number=%d show=%d frame_type=%d base_q=%d\n",
         inspect_ctx.frame_data.frame_number,
         inspect_ctx.frame_data.immediate_output_picture,
         inspect_ctx.frame_data.frame_type, inspect_ctx.frame_data.base_qindex);
  printf("size=%dx%d render=%dx%d bit_depth=%d tile_mi=%dx%d sb_size=%d\n",
         inspect_ctx.frame_data.width, inspect_ctx.frame_data.height,
         inspect_ctx.frame_data.render_width, inspect_ctx.frame_data.render_height,
         inspect_ctx.frame_data.bit_depth, inspect_ctx.frame_data.tile_mi_cols,
         inspect_ctx.frame_data.tile_mi_rows,
         inspect_ctx.frame_data.superblock_size);

  printf(
      "mi00 mode=%d uv_mode=%d sb_type=%d sb_type_chroma=%d skip=%d tx_size=%d tx_type=%d cfl_alpha_idx=%d cfl_alpha_sign=%d segment_id=%d qindex=%d\n",
      mi00->mode, mi00->uv_mode, mi00->sb_type, mi00->sb_type_chroma,
      mi00->skip, mi00->tx_size, mi00->tx_type, mi00->cfl_alpha_idx,
      mi00->cfl_alpha_sign, mi00->segment_id, mi00->current_qindex);

  printf("luma_tree:\n");
  print_tree(sb00->partition_tree_luma, 0);
  printf("chroma_tree:\n");
  print_tree(sb00->partition_tree_chroma, 0);

  ifd_clear(&inspect_ctx.frame_data);
  if (avm_codec_destroy(&codec)) die_codec(&codec, "Failed to destroy codec.");
  avm_video_reader_close(reader);
  return EXIT_SUCCESS;
}
