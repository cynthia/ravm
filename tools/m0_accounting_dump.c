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
#include "av2/decoder/accounting.h"
#include "common/tools_common.h"
#include "common/video_reader.h"

static const char *g_exec_name = "m0_accounting_dump";

void usage_exit(void) {
  fprintf(stderr, "Usage: %s <input.ivf> [limit]\n", g_exec_name);
  exit(EXIT_FAILURE);
}

static void m0_usage_exit(const char *exec_name) {
  fprintf(stderr, "Usage: %s <input.ivf> [limit]\n", exec_name);
  exit(EXIT_FAILURE);
}

int main(int argc, char **argv) {
#if !CONFIG_ACCOUNTING
  (void)argc;
  (void)argv;
  fprintf(stderr, "CONFIG_ACCOUNTING is disabled in this build.\n");
  return EXIT_FAILURE;
#else
  const char *exec_name = argv[0];
  g_exec_name = exec_name;
  if (argc < 2 || argc > 3) m0_usage_exit(exec_name);

  int limit = 64;
  if (argc == 3) limit = atoi(argv[2]);
  if (limit <= 0) limit = 64;

  AvxVideoReader *reader = avm_video_reader_open(argv[1]);
  if (!reader) die("Failed to open %s", argv[1]);

  const AvxVideoInfo *video_info = avm_video_reader_get_info(reader);
  avm_codec_iface_t *decoder;
  (void)video_info;
  if (get_avm_decoder_count() < 1) die("No decoder interfaces are available.");
  decoder = get_avm_decoder_by_index(0);

  avm_codec_ctx_t codec;
  if (avm_codec_dec_init(&codec, decoder, NULL, 0))
    die("Failed to initialize decoder.");

  if (!avm_video_reader_read_frame(reader))
    die("Failed to read first frame.");

  size_t frame_size = 0;
  const unsigned char *frame = avm_video_reader_get_frame(reader, &frame_size);
  const int decode_failed = avm_codec_decode(&codec, frame, frame_size, NULL);

  Accounting *accounting = NULL;
  if (avm_codec_control(&codec, AV2_GET_ACCOUNTING, &accounting)) {
    if (decode_failed) die_codec(&codec, "Failed to decode frame.");
    die_codec(&codec, "Failed to get accounting.");
  }
  if (accounting == NULL) {
    if (decode_failed) die_codec(&codec, "Failed to decode frame.");
    die("Decoder returned a null Accounting pointer.");
  }

  printf("num_symbols=%d\n", accounting->syms.num_syms);
  printf("num_symbol_types=%d\n", accounting->syms.dictionary.num_strs);

  const int num_syms =
      limit < accounting->syms.num_syms ? limit : accounting->syms.num_syms;
  for (int i = 0; i < num_syms; ++i) {
    AccountingSymbol *sym = &accounting->syms.syms[i];
    AccountingSymbolInfo *sym_info =
        &accounting->syms.dictionary.acct_infos[sym->id];
    printf(
        "%03d ctx=(%d,%d,%d) id=%u bits=%llu value=%d mode=%d func=%s file=%s:%d",
        i, sym->context.x, sym->context.y, sym->context.tree_type, sym->id,
        (unsigned long long)sym->bits, sym->value, sym->coding_mode,
        sym_info->c_func, sym_info->c_file, sym_info->c_line);
    for (int tag = 0; tag < AVM_ACCOUNTING_MAX_TAGS; ++tag) {
      if (sym_info->tags[tag] == NULL) break;
      printf(" tag%d=%s", tag, sym_info->tags[tag]);
    }
    putchar('\n');
  }

  if (decode_failed) {
    die_codec(&codec, "Failed to decode frame.");
  }

  if (avm_codec_destroy(&codec)) die_codec(&codec, "Failed to destroy codec.");
  avm_video_reader_close(reader);
  return EXIT_SUCCESS;
#endif
}
