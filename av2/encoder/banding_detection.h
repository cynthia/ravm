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
#ifndef AVM_AV2_COMMON_BANDING_DETECTION_H_
#define AVM_AV2_COMMON_BANDING_DETECTION_H_

#include "avm/avm_integer.h"
#include "av2/common/av2_common_int.h"
#include "av2/common/reconinter.h"
#include "av2/common/banding_metadata.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SWAP_FLOATS(x, y) \
    {                     \
        float temp = x;   \
        x = y;            \
        y = temp;         \
    }

/* Window size to compute CAMBI: 65 corresponds to approximately 1 degree at 4k scale */
#define CAMBI_DEFAULT_WINDOW_SIZE (65)

/* Encoder banding detection thresholds */
#define CAMBI_DIFF_THRESHOLD_8b 4
#define CAMBI_SOURCE_THRESHOLD_8b 3

#define CAMBI_DIFF_THRESHOLD_10b 3
#define CAMBI_SOURCE_THRESHOLD_10b 2

/* Visibility threshold for luminance ΔL < tvi_threshold*L_mean for BT.1886 */
#define CAMBI_TVI (0.019)

/* Max log contrast luma levels */
#define CAMBI_DEFAULT_MAX_LOG_CONTRAST (2)

/* Window size for CAMBI */
#define CAMBI_MIN_WIDTH (192)
#define CAMBI_MAX_WIDTH (4096)

#define CAMBI_NUM_SCALES (5)

/* Ratio of pixels for computation, must be 0 > topk >= 1.0 */
#define CAMBI_DEFAULT_TOPK_POOLING (0.6)

/* Spatial mask filter size for CAMBI */
#define CAMBI_MASK_FILTER_SIZE (7)

#define CLAMP(x, low, high) (((x) > (high)) ? (high) : (((x) < (low)) ? (low) : (x)))

int avm_band_detection_init(BandDetectInfo *const dbi, const int frame_width,
                           const int frame_height, const int bit_depth);

void avm_band_detection_close(BandDetectInfo *const dbi);

void avm_band_detection(const YV12_BUFFER_CONFIG *frame,
                        const YV12_BUFFER_CONFIG *ref, AV2_COMMON *cm,
                        MACROBLOCKD *xd,
                        avm_banding_hints_metadata_t *band_metadata);

#ifdef __cplusplus
}  // extern "C"
#endif
#endif  // AVM_AV2_COMMON_BANDING_DETECTION_H_
