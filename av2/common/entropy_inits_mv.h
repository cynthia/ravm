/*
 * Copyright (c) 2024, Alliance for Open Media. All rights reserved
 *
 * This source code is subject to the terms of the BSD 3-Clause Clear License
 * and the Alliance for Open Media Patent License 1.0. If the BSD 3-Clause Clear
 * License was not distributed with this source code in the LICENSE file, you
 * can obtain it at aomedia.org/license/software-license/bsd-3-c-c/.  If the
 * Alliance for Open Media Patent License 1.0 was not distributed with this
 * source code in the PATENTS file, you can obtain it at
 * aomedia.org/license/patent-license/.
 */

#ifndef AVM_AV2_COMMON_ENTROPY_INITS_MV_H_
#define AVM_AV2_COMMON_ENTROPY_INITS_MV_H_

#include "config/avm_config.h"
#include "av2/common/entropymv.h"

#ifdef __cplusplus
extern "C" {
#endif

static const nmv_context default_nmv_context = {
  { AVM_CDF2(31579), AVM_PARA2(-1, 0, 0) },
  {
      { AVM_CDF5(4460, 12999, 22505, 30840), AVM_PARA5(-1, -2, 1) },
      { AVM_CDF6(7519, 18907, 25563, 29875, 31983), AVM_PARA6(-1, 0, -2) },
      { AVM_CDF6(5461, 10923, 16384, 21845, 27307), AVM_PARA6(0, 0, 0) },
      { AVM_CDF7(8680, 13723, 18208, 22686, 26722, 30020),
        AVM_PARA7(0, -1, 0) },
      { AVM_CDF7(4324, 15300, 23690, 28697, 31282, 32359),
        AVM_PARA7(0, 0, -1) },
      { AVM_CDF8(7497, 17301, 23848, 27438, 29395, 30879, 32003),
        AVM_PARA8(-1, -1, -1) },
      { AVM_CDF8(10667, 20239, 25883, 29670, 31400, 32153, 32579),
        AVM_PARA8(0, 0, 0) },
  },
  {
      { AVM_CDF6(21329, 30564, 32589, 32649, 32708), AVM_PARA6(-2, 0, 0) },
      { AVM_CDF6(24250, 31806, 32676, 32722, 32732), AVM_PARA6(-2, 0, 0) },
      { AVM_CDF7(4681, 9362, 14043, 18725, 23406, 28087), AVM_PARA7(0, 0, 0) },
      { AVM_CDF7(19978, 30160, 32564, 32732, 32736, 32740),
        AVM_PARA7(0, 0, -1) },
      { AVM_CDF8(19707, 28414, 31240, 31648, 32692, 32717, 32721),
        AVM_PARA8(-1, 0, 0) },
      { AVM_CDF8(18469, 27427, 31562, 32652, 32724, 32728, 32732),
        AVM_PARA8(-1, -1, -1) },
      { AVM_CDF8(17810, 25196, 29372, 31953, 32564, 32720, 32724),
        AVM_PARA8(-2, -1, 0) },
  },
  { AVM_CDF2(16384), AVM_PARA2(0, 0, 0) },
  {
      { AVM_CDF2(14587), AVM_PARA2(-1, -2, -1) },
      { AVM_CDF2(20966), AVM_PARA2(1, 0, 0) },
  },
  { AVM_CDF2(13189), AVM_PARA2(0, 0, 0) },
  {
      {
          { AVM_CDF2(17943), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(18934), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(18928), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(18696), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(19044), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(20362), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(20426), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(22563), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(22190), AVM_PARA2(1, 1, 1) },
          { AVM_CDF2(23458), AVM_PARA2(1, 1, 0) },
          { AVM_CDF2(26227), AVM_PARA2(0, 0, -2) },
          { AVM_CDF2(30765), AVM_PARA2(-2, 0, 0) },
          { AVM_CDF2(16384), AVM_PARA2(0, 0, 0) },
          { AVM_CDF2(16384), AVM_PARA2(0, 0, 0) },
          { AVM_CDF2(16384), AVM_PARA2(0, 0, 0) },
          { AVM_CDF2(16384), AVM_PARA2(0, 0, 0) },
      },
  },
  {
      { AVM_CDF2(5663), AVM_PARA2(-1, 0, 0) },
      { AVM_CDF2(4856), AVM_PARA2(1, 1, 0) },
  },
  {
      { AVM_CDF2(13445), AVM_PARA2(0, 0, -1) },
      { AVM_CDF2(13541), AVM_PARA2(0, 0, -1) },
      { AVM_CDF2(14045), AVM_PARA2(0, 0, -1) },
      { AVM_CDF2(12888), AVM_PARA2(-1, -1, -1) },
  },
  { AVM_CDF4(4, 17705, 32748), AVM_PARA4(0, 0, -1) },
  {
      {
          { AVM_CDF8(10549, 15298, 16241, 22533, 27449, 30520, 32080),
            AVM_PARA8(-1, 0, -1) },
      },
      {
          { AVM_CDF8(9414, 14965, 15966, 22465, 27468, 30628, 32144),
            AVM_PARA8(-1, 0, -1) },
      },
  },
};

#ifdef __cplusplus
}  // extern "C"
#endif
#endif  // AVM_AV2_COMMON_ENTROPY_INITS_MV_H_
