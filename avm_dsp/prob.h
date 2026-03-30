/*
 * Copyright (c) 2021, Alliance for Open Media. All rights reserved
 *
 * This source code is subject to the terms of the BSD 3-Clause Clear License
 * and the Alliance for Open Media Patent License 1.0. If the BSD 3-Clause Clear
 * License was not distributed with this source code in the LICENSE file, you
 * can obtain it at aomedia.org/license/software-license/bsd-3-c-c/.  If the
 * Alliance for Open Media Patent License 1.0 was not distributed with this
 * source code in the PATENTS file, you can obtain it at
 * aomedia.org/license/patent-license/.
 */

#ifndef AVM_AVM_DSP_PROB_H_
#define AVM_AVM_DSP_PROB_H_

#include <assert.h>
#include <stdio.h>

#include "config/avm_config.h"

#include "avm_dsp/avm_dsp_common.h"
#include "avm_dsp/entcode.h"
#include "avm_ports/bitops.h"
#include "avm_ports/mem.h"

typedef uint16_t avm_cdf_prob;

#define EC_PROB_SHIFT 7
#define EC_MIN_PROB 4  // must be <= (1<<EC_PROB_SHIFT)/16

#define CDF_SIZE(x) ((x) + 4)

// AVM_PARAn(a, b, c): emit 3 pre-computed rate offsets for time intervals 0,
// 1, 2.  a, b, c are raw PARA triplet values from training (range {-2..+1}).
#define AVM_PARA2(a, b, c) ((a) + 2), ((b) + 3), ((c) + 4)
#define AVM_PARA3(a, b, c) ((a) + 2), ((b) + 3), ((c) + 4)
#define AVM_PARA4(a, b, c) ((a) + 3), ((b) + 4), ((c) + 5)
#define AVM_PARA5(a, b, c) ((a) + 3), ((b) + 4), ((c) + 5)
#define AVM_PARA6(a, b, c) ((a) + 3), ((b) + 4), ((c) + 5)
#define AVM_PARA7(a, b, c) ((a) + 3), ((b) + 4), ((c) + 5)
#define AVM_PARA8(a, b, c) ((a) + 3), ((b) + 4), ((c) + 5)
#define CDF_PROB_BITS 15
#define CDF_PROB_TOP (1 << CDF_PROB_BITS)
#define CDF_INIT_TOP 32768
#define CDF_SHIFT (15 - CDF_PROB_BITS)
/*The value stored in an iCDF is CDF_PROB_TOP minus the actual cumulative
  probability (an "inverse" CDF).
  This function converts from one representation to the other (and is its own
  inverse).*/
#define AVM_ICDF(x) (CDF_PROB_TOP - (x))

#if CDF_SHIFT == 0

#define AVM_CDF2(a0) AVM_ICDF(a0), AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF3(a0, a1) AVM_ICDF(a0), AVM_ICDF(a1), AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF4(a0, a1, a2) \
  AVM_ICDF(a0), AVM_ICDF(a1), AVM_ICDF(a2), AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF5(a0, a1, a2, a3) \
  AVM_ICDF(a0)                   \
  , AVM_ICDF(a1), AVM_ICDF(a2), AVM_ICDF(a3), AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF6(a0, a1, a2, a3, a4)                        \
  AVM_ICDF(a0)                                              \
  , AVM_ICDF(a1), AVM_ICDF(a2), AVM_ICDF(a3), AVM_ICDF(a4), \
      AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF7(a0, a1, a2, a3, a4, a5)                                  \
  AVM_ICDF(a0)                                                            \
  , AVM_ICDF(a1), AVM_ICDF(a2), AVM_ICDF(a3), AVM_ICDF(a4), AVM_ICDF(a5), \
      AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF8(a0, a1, a2, a3, a4, a5, a6)                              \
  AVM_ICDF(a0)                                                            \
  , AVM_ICDF(a1), AVM_ICDF(a2), AVM_ICDF(a3), AVM_ICDF(a4), AVM_ICDF(a5), \
      AVM_ICDF(a6), AVM_ICDF(CDF_PROB_TOP), 0
#else
#define AVM_CDF2(a0)                                         \
  AVM_ICDF((((a0) - 1) * ((CDF_INIT_TOP >> CDF_SHIFT) - 2) + \
            ((CDF_INIT_TOP - 2) >> 1)) /                     \
               ((CDF_INIT_TOP - 2)) +                        \
           1)                                                \
  , AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF3(a0, a1)                                         \
  AVM_ICDF((((a0) - 1) * ((CDF_INIT_TOP >> CDF_SHIFT) - 3) +     \
            ((CDF_INIT_TOP - 3) >> 1)) /                         \
               ((CDF_INIT_TOP - 3)) +                            \
           1)                                                    \
  ,                                                              \
      AVM_ICDF((((a1) - 2) * ((CDF_INIT_TOP >> CDF_SHIFT) - 3) + \
                ((CDF_INIT_TOP - 3) >> 1)) /                     \
                   ((CDF_INIT_TOP - 3)) +                        \
               2),                                               \
      AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF4(a0, a1, a2)                                     \
  AVM_ICDF((((a0) - 1) * ((CDF_INIT_TOP >> CDF_SHIFT) - 4) +     \
            ((CDF_INIT_TOP - 4) >> 1)) /                         \
               ((CDF_INIT_TOP - 4)) +                            \
           1)                                                    \
  ,                                                              \
      AVM_ICDF((((a1) - 2) * ((CDF_INIT_TOP >> CDF_SHIFT) - 4) + \
                ((CDF_INIT_TOP - 4) >> 1)) /                     \
                   ((CDF_INIT_TOP - 4)) +                        \
               2),                                               \
      AVM_ICDF((((a2) - 3) * ((CDF_INIT_TOP >> CDF_SHIFT) - 4) + \
                ((CDF_INIT_TOP - 4) >> 1)) /                     \
                   ((CDF_INIT_TOP - 4)) +                        \
               3),                                               \
      AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF5(a0, a1, a2, a3)                                 \
  AVM_ICDF((((a0) - 1) * ((CDF_INIT_TOP >> CDF_SHIFT) - 5) +     \
            ((CDF_INIT_TOP - 5) >> 1)) /                         \
               ((CDF_INIT_TOP - 5)) +                            \
           1)                                                    \
  ,                                                              \
      AVM_ICDF((((a1) - 2) * ((CDF_INIT_TOP >> CDF_SHIFT) - 5) + \
                ((CDF_INIT_TOP - 5) >> 1)) /                     \
                   ((CDF_INIT_TOP - 5)) +                        \
               2),                                               \
      AVM_ICDF((((a2) - 3) * ((CDF_INIT_TOP >> CDF_SHIFT) - 5) + \
                ((CDF_INIT_TOP - 5) >> 1)) /                     \
                   ((CDF_INIT_TOP - 5)) +                        \
               3),                                               \
      AVM_ICDF((((a3) - 4) * ((CDF_INIT_TOP >> CDF_SHIFT) - 5) + \
                ((CDF_INIT_TOP - 5) >> 1)) /                     \
                   ((CDF_INIT_TOP - 5)) +                        \
               4),                                               \
      AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF6(a0, a1, a2, a3, a4)                             \
  AVM_ICDF((((a0) - 1) * ((CDF_INIT_TOP >> CDF_SHIFT) - 6) +     \
            ((CDF_INIT_TOP - 6) >> 1)) /                         \
               ((CDF_INIT_TOP - 6)) +                            \
           1)                                                    \
  ,                                                              \
      AVM_ICDF((((a1) - 2) * ((CDF_INIT_TOP >> CDF_SHIFT) - 6) + \
                ((CDF_INIT_TOP - 6) >> 1)) /                     \
                   ((CDF_INIT_TOP - 6)) +                        \
               2),                                               \
      AVM_ICDF((((a2) - 3) * ((CDF_INIT_TOP >> CDF_SHIFT) - 6) + \
                ((CDF_INIT_TOP - 6) >> 1)) /                     \
                   ((CDF_INIT_TOP - 6)) +                        \
               3),                                               \
      AVM_ICDF((((a3) - 4) * ((CDF_INIT_TOP >> CDF_SHIFT) - 6) + \
                ((CDF_INIT_TOP - 6) >> 1)) /                     \
                   ((CDF_INIT_TOP - 6)) +                        \
               4),                                               \
      AVM_ICDF((((a4) - 5) * ((CDF_INIT_TOP >> CDF_SHIFT) - 6) + \
                ((CDF_INIT_TOP - 6) >> 1)) /                     \
                   ((CDF_INIT_TOP - 6)) +                        \
               5),                                               \
      AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF7(a0, a1, a2, a3, a4, a5)                         \
  AVM_ICDF((((a0) - 1) * ((CDF_INIT_TOP >> CDF_SHIFT) - 7) +     \
            ((CDF_INIT_TOP - 7) >> 1)) /                         \
               ((CDF_INIT_TOP - 7)) +                            \
           1)                                                    \
  ,                                                              \
      AVM_ICDF((((a1) - 2) * ((CDF_INIT_TOP >> CDF_SHIFT) - 7) + \
                ((CDF_INIT_TOP - 7) >> 1)) /                     \
                   ((CDF_INIT_TOP - 7)) +                        \
               2),                                               \
      AVM_ICDF((((a2) - 3) * ((CDF_INIT_TOP >> CDF_SHIFT) - 7) + \
                ((CDF_INIT_TOP - 7) >> 1)) /                     \
                   ((CDF_INIT_TOP - 7)) +                        \
               3),                                               \
      AVM_ICDF((((a3) - 4) * ((CDF_INIT_TOP >> CDF_SHIFT) - 7) + \
                ((CDF_INIT_TOP - 7) >> 1)) /                     \
                   ((CDF_INIT_TOP - 7)) +                        \
               4),                                               \
      AVM_ICDF((((a4) - 5) * ((CDF_INIT_TOP >> CDF_SHIFT) - 7) + \
                ((CDF_INIT_TOP - 7) >> 1)) /                     \
                   ((CDF_INIT_TOP - 7)) +                        \
               5),                                               \
      AVM_ICDF((((a5) - 6) * ((CDF_INIT_TOP >> CDF_SHIFT) - 7) + \
                ((CDF_INIT_TOP - 7) >> 1)) /                     \
                   ((CDF_INIT_TOP - 7)) +                        \
               6),                                               \
      AVM_ICDF(CDF_PROB_TOP), 0
#define AVM_CDF8(a0, a1, a2, a3, a4, a5, a6)                     \
  AVM_ICDF((((a0) - 1) * ((CDF_INIT_TOP >> CDF_SHIFT) - 8) +     \
            ((CDF_INIT_TOP - 8) >> 1)) /                         \
               ((CDF_INIT_TOP - 8)) +                            \
           1)                                                    \
  ,                                                              \
      AVM_ICDF((((a1) - 2) * ((CDF_INIT_TOP >> CDF_SHIFT) - 8) + \
                ((CDF_INIT_TOP - 8) >> 1)) /                     \
                   ((CDF_INIT_TOP - 8)) +                        \
               2),                                               \
      AVM_ICDF((((a2) - 3) * ((CDF_INIT_TOP >> CDF_SHIFT) - 8) + \
                ((CDF_INIT_TOP - 8) >> 1)) /                     \
                   ((CDF_INIT_TOP - 8)) +                        \
               3),                                               \
      AVM_ICDF((((a3) - 4) * ((CDF_INIT_TOP >> CDF_SHIFT) - 8) + \
                ((CDF_INIT_TOP - 8) >> 1)) /                     \
                   ((CDF_INIT_TOP - 8)) +                        \
               4),                                               \
      AVM_ICDF((((a4) - 5) * ((CDF_INIT_TOP >> CDF_SHIFT) - 8) + \
                ((CDF_INIT_TOP - 8) >> 1)) /                     \
                   ((CDF_INIT_TOP - 8)) +                        \
               5),                                               \
      AVM_ICDF((((a5) - 6) * ((CDF_INIT_TOP >> CDF_SHIFT) - 8) + \
                ((CDF_INIT_TOP - 8) >> 1)) /                     \
                   ((CDF_INIT_TOP - 8)) +                        \
               6),                                               \
      AVM_ICDF((((a6) - 7) * ((CDF_INIT_TOP >> CDF_SHIFT) - 8) + \
                ((CDF_INIT_TOP - 8) >> 1)) /                     \
                   ((CDF_INIT_TOP - 8)) +                        \
               7),                                               \
      AVM_ICDF(CDF_PROB_TOP), 0
#endif

static INLINE uint8_t get_prob(unsigned int num, unsigned int den) {
  assert(den != 0);
  {
    const int p = (int)(((uint64_t)num * 256 + (den >> 1)) / den);
    // (p > 255) ? 255 : (p < 1) ? 1 : p;
    const int clipped_prob = p | ((255 - p) >> 23) | (p == 0);
    return (uint8_t)clipped_prob;
  }
}

static INLINE void update_cdf(avm_cdf_prob *cdf, int8_t val, int nsymbs) {
  int i, tmp;
  assert(nsymbs < 17);
  const int time_interval = cdf[nsymbs] > 31 ? 2 : cdf[nsymbs] > 15 ? 1 : 0;
  const int rate = 2 + cdf[nsymbs + 1 + time_interval];
  tmp = AVM_ICDF(0);

  // Single loop (faster)
  for (i = 0; i < nsymbs - 1; ++i) {
    tmp = (i == val) ? 0 : tmp;
    if (tmp < cdf[i]) {
      cdf[i] -= ((cdf[i] - tmp) >> rate);
    } else {
      cdf[i] += ((tmp - cdf[i]) >> rate);
    }
  }
  cdf[nsymbs] += (cdf[nsymbs] < 32);
}

// Scale the CDF to match the range value stored in the entropy decoder.
static INLINE unsigned od_ec_prob_scale(uint16_t p, unsigned r, int n,
                                        int nsym) {
  int rr = r >> 8;
  int pp = p >> EC_PROB_SHIFT;
  pp <<= 4;
  pp += av2_prob_inc_tbl[nsym - 2][n];
  return ((rr * pp >> (7 - EC_PROB_SHIFT - CDF_SHIFT + 1 + 6)) << 3);
}

// Adjust probability to more closely match the scaled prob used in
// od_ec_prob_scale()
static INLINE unsigned get_adjusted_prob(uint16_t p, int n, int nsym) {
  int adj_prob = (p >> EC_PROB_SHIFT) << EC_PROB_SHIFT;
  int inc = av2_prob_inc_tbl[nsym - 2][n];
  adj_prob += inc << (EC_PROB_SHIFT - 4);
  return adj_prob;
}

#endif  // AVM_AVM_DSP_PROB_H_
