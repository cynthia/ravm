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

#include "config/avm_dsp_rtcd.h"
#include "av2/common/av2_common_int.h"
#include "av2/encoder/banding_detection.h"
#include "avm_mem/avm_mem.h"

/* CAMBI preprocessing functions */
static void copy_8b_to_10b_buffer(uint16_t *data, int stride,
                                  unsigned in_w, unsigned in_h,
                                  uint16_t *out_data, int out_stride,
                                  unsigned out_w, unsigned out_h) {

  // if the input and output sizes are the same
  if (in_w == out_w && in_h == out_h) {
    for (unsigned i = 0; i < out_h; i++)
      for (unsigned j = 0; j < out_w; j++)
        out_data[i * out_stride + j] = data[i * stride + j] << 2;
  } else {
    printf("Error in copy_8b_to_10b_buffer: different size\n");
  }
}

static void copy_10b_buffer(uint16_t *data, int stride,
                            unsigned in_w, unsigned in_h,
                            uint16_t *out_data, int out_stride,
                            unsigned out_w, unsigned out_h) {
  if (in_w == out_w && in_h == out_h) {
    for (unsigned i = 0; i < out_h; i++) {
      memcpy(out_data, data, in_w * sizeof(uint16_t));
      data += stride;
      out_data += out_stride;
    }
  } else {
    printf("Error in copy_10b_buffer: different size\n");
  }
}

static void anti_dithering_filter(uint16_t *data, int stride,
                                  int width, int height) {

  for (int i = 0; i < height - 1; i++) {
    for (int j = 0; j < width - 1; j++) {
      data[i * stride + j] = (data[i * stride + j] +
                              data[i * stride + j + 1] +
                              data[(i + 1) * stride + j] +
                              data[(i + 1) * stride + j + 1]) >> 2;
    }

    // Last column
    int j = width - 1;
    data[i * stride + j] = (data[i * stride + j] +
                            data[(i + 1) * stride + j]) >> 1;
  }

  // Last row
  int i = height - 1;
  for (int j = 0; j < width - 1; j++) {
    data[i * stride + j] = (data[i * stride + j] +
                            data[i * stride + j + 1]) >> 1;
  }
}

void cambi_preprocessing(uint16_t *data, int stride, int in_w, int in_h,
                         int bit_depth, uint16_t *out_data, int out_stride,
                         int out_w, int out_h) {
  if (bit_depth == 8) {
    copy_8b_to_10b_buffer(data, stride, in_w, in_h,
                          out_data, out_stride, out_w, out_h);
    anti_dithering_filter(out_data, out_stride, out_w, out_h);
  }
  else {
    copy_10b_buffer(data, stride, in_w, in_h, out_data, out_stride,
                    out_w, out_h);
  }
}


/* CAMBI processing functions */
static inline uint16_t get_pixels_in_window(uint16_t window_length) {
  uint16_t odd_length = 2 * (window_length >> 1) + 1;
  return odd_length * odd_length;
}

static inline uint16_t adjust_cambi_window_size(uint16_t size,
                                                unsigned width,
                                                unsigned height) {
  // Adjustment with (input_width + input_height) / (4K_WIDTH + 4K_HEIGHT)
  return CLAMP(((size * (width + height)) / 375) >> 4,
               5, CAMBI_DEFAULT_WINDOW_SIZE);
}

void set_cambi_window(BandDetectInfo *const dbi) {
  dbi->window_size = CAMBI_DEFAULT_WINDOW_SIZE;
  dbi->window_size =
      adjust_cambi_window_size(dbi->window_size, dbi->stride, dbi->height);
  dbi->pixels_in_window = get_pixels_in_window(dbi->window_size);
}

static void cambi_decimate(uint16_t *data, int stride,
                           unsigned width, unsigned height) {
  for (unsigned i = 0; i < height; i++) {
    for (unsigned j = 0; j < width; j++) {
      data[i * stride + j] = data[(i << 1) * stride + (j << 1)];
    }
  }
}

/* Spatial mask functions */
static inline uint16_t ceil_log2(uint32_t num) {
  if (num==0)
    return 0;

  uint32_t tmp = num - 1;
  uint16_t shift = 0;
  while (tmp>0) {
    tmp >>= 1;
    shift += 1;
  }
  return shift;
}

uint16_t cambi_get_mask_index(int input_width, int input_height,
                              uint16_t filter_size) {
  uint32_t shifted_wh = (input_width >> 6) * (input_height >> 6);
  return (filter_size * filter_size + 3 * (ceil_log2(shifted_wh) - 11) - 1)>>1;
}

static inline bool get_derivative_data(const uint16_t *data,
                                       int i, int j, int str) {
  return (data[i * str + j] == data[(i + 1) * str + j]) &&
         (data[i * str + j] == data[i * str + j + 1]);
}

/*
* This function calculates the horizontal and vertical derivatives of the image
* using 2x1 and 1x2 kernels.
 */
static void get_spatial_mask_for_index(BandDetectInfo *dbi, uint16_t mask_index,
                                       uint16_t filter_size, int width,
                                       int height) {
  uint16_t pad_size = filter_size >> 1;
  uint16_t *image_data = dbi->frame;
  uint16_t *mask_data = dbi->mask;
  int stride = dbi->stride;
  uint32_t *dp = dbi->buffers.mask_dp;

  int dp_width = width + 2 * pad_size + 1;
  int dp_height = 2 * pad_size + 2;
  memset(dp, 0, dp_width * dp_height * sizeof(uint32_t));

  // Initial computation: fill dp except for the last row
  for (int i = 0; i < pad_size; i++) {
    int cur_row_start = (i + pad_size + 1) * dp_width;
    int prev_row_start = cur_row_start - dp_width;
    int curr_col = pad_size + 1;
    for (int j = 0; j < width + pad_size; j++, curr_col++) {
      int value = (i < height-1 && j < width-1 ? get_derivative_data(image_data, i, j, stride) : 0);
      dp[cur_row_start + curr_col] =
          value
          + dp[prev_row_start + curr_col]
          + dp[cur_row_start + curr_col - 1]
          - dp[prev_row_start + curr_col - 1];
    }
  }

  // Start from the last row in the dp matrix
  int curr_row = dp_height - 1;
  int prev_row = dp_height - 2;
  int bottom = 2 * pad_size;
  for (int i = pad_size; i < height + pad_size; i++) {
    // First compute the values of dp for curr_row
    int curr_col = pad_size + 1;
    for (int j = 0; j < width + pad_size; j++, curr_col++) {
      int value = (i < height-1 && j < width-1 ? get_derivative_data(image_data, i, j, stride) : 0);
      dp[curr_row * dp_width + curr_col] =
          value
          + dp[prev_row * dp_width + curr_col]
          + dp[curr_row * dp_width + curr_col - 1]
          - dp[prev_row * dp_width + curr_col - 1];
    }
    prev_row = curr_row;
    curr_row = curr_row==(dp_height-1) ? 0 : curr_row+1;
    bottom = bottom==(dp_height-1) ? 0 : bottom+1;

    // Then use the values to compute the square sum for the curr computed row.
    int right = 2*pad_size + 1;
    int top = curr_row;
    for (int left = 0; left < width; left++, right++) {
      int result =
          dp[bottom * dp_width + right]
          - dp[bottom * dp_width + left]
          - dp[top * dp_width + right]
          + dp[top * dp_width + left];
      mask_data[(i - pad_size) * stride + left] = (result > mask_index);
    }
  }
}

void cambi_get_spatial_mask(BandDetectInfo *dbi, int width, int height) {
  const uint16_t filter_size = CAMBI_MASK_FILTER_SIZE;
  uint16_t mask_index = cambi_get_mask_index(width, height, filter_size);
  get_spatial_mask_for_index(dbi, mask_index, filter_size, width, height);
}

static inline uint16_t min3(uint16_t a, uint16_t b, uint16_t c) {
  if (a <= b && a <= c) return a;
  if (b <= c) return b;
  return c;
}

static inline uint16_t mode3(uint16_t a, uint16_t b, uint16_t c) {
  if (a == b || a == c) return a;
  if (b == c) return b;
  return min3(a, b, c);
}

void cambi_filter_mode(BandDetectInfo *dbi, int width, int height) {
  uint16_t *data = dbi->frame;
  ptrdiff_t stride = dbi->stride;
  uint16_t *buffer = dbi->buffers.filter_mode_buffer;

  for (int i = 0; i < height; i++) {
    int curr_line = i % 3;
    buffer[curr_line * width + 0] = data[i * stride + 0];
    for (int j = 1; j < width - 1; j++) {
      buffer[curr_line * width + j] = mode3(data[i * stride + j - 1], data[i * stride + j], data[i * stride + j + 1]);
    }
    buffer[curr_line * width + width - 1] = data[i * stride + width - 1];

    if (i > 1) {
      for (int j = 0; j < width; j++) {
        data[(i - 1) * stride + j] = mode3(buffer[0 * width + j], buffer[1 * width + j], buffer[2 * width + j]);
      }
    }
  }
}

static inline void increment_range(uint16_t *arr, int left, int right) {
  for (int i = left; i < right; i++) {
    arr[i]++;
  }
}

static inline void decrement_range(uint16_t *arr, int left, int right) {
  for (int i = left; i < right; i++) {
    arr[i]--;
  }
}

static inline void cambi_histogram_sub_edge(uint16_t *histograms,
                                     uint16_t *image, uint16_t *mask,
                                     int i, int j, int width,
                                     ptrdiff_t stride, uint16_t pad_size,
                                     const uint16_t num_diffs) {
  long int index = (i - pad_size - 1) * stride + j;
  if (mask[index]) {
    uint16_t val = image[index] + num_diffs;
    decrement_range(&histograms[val * width], AVMMAX(j - pad_size, 0),
                    AVMMIN(j + pad_size + 1, width));
  }
}

static inline void cambi_histogram_sub(uint16_t *histograms, const uint16_t *image,
                                const uint16_t *mask, int i, int j, int width,
                                ptrdiff_t stride, uint16_t pad_size,
                                uint16_t num_diffs) {
  long int index = (i - pad_size - 1) * stride + j;
  if (mask[index]) {
    uint16_t val = image[index] + num_diffs;
    decrement_range(&histograms[val * width], j - pad_size, j + pad_size + 1);
  }
}

static inline void cambi_histogram_add_edge(uint16_t *histograms, uint16_t *image,
                                     uint16_t *mask, int i, int j, int width,
                                     ptrdiff_t stride, uint16_t pad_size,
                                     const uint16_t num_diffs) {
  long int index = (i + pad_size) * stride + j;
  if (mask[index]) {
    uint16_t val = image[index] + num_diffs;
    increment_range(&histograms[val * width], AVMMAX(j - pad_size, 0),
                    AVMMIN(j + pad_size + 1, width));
  }
}

static inline void cambi_histogram_add(uint16_t *histograms, const uint16_t *image,
                                const uint16_t *mask, int i, int j, int width,
                                ptrdiff_t stride, uint16_t pad_size,
                                uint16_t num_diffs) {
  long int index = (i + pad_size) * stride + j;
  if (mask[index]) {
    uint16_t val = image[index] + num_diffs;
    increment_range(&histograms[val * width], j - pad_size, j + pad_size + 1);
  }
}

static inline void cambi_histogram_add_edge_first_pass(uint16_t *histograms,
                                                uint16_t *image, uint16_t *mask,
                                                int i, int j, int width,
                                                ptrdiff_t stride,
                                                uint16_t pad_size,
                                                const uint16_t num_diffs) {
  long int index = i * stride + j;
  if (mask[index]) {
    uint16_t val = image[index] + num_diffs;
    increment_range(&histograms[val * width], AVMMAX(j - pad_size, 0),
                    AVMMIN(j + pad_size + 1, width));
  }
}

static inline void cambi_histogram_add_first_pass(uint16_t *histograms,
                                           uint16_t *image, uint16_t *mask,
                                           int i, int j, int width,
                                           ptrdiff_t stride, uint16_t pad_size,
                                           const uint16_t num_diffs) {
  long int index = i * stride + j;
  if (mask[index]) {
    uint16_t val = image[index] + num_diffs;
    increment_range(&histograms[val * width], j - pad_size, j + pad_size + 1);
  }
}

static float c_value_pixel(const uint16_t *histograms, uint16_t value,
                           const int *diff_weights, uint16_t num_diffs,
                           const uint16_t *tvi_thresholds,
                           int histogram_col, int histogram_width) {
  uint16_t p_0 = histograms[value * histogram_width + histogram_col];
  float val;
  float c_value = (float) 0.0;
  for (uint16_t d = 0; d < num_diffs; d++) {
    if (value <= tvi_thresholds[d]) {
      uint16_t p_1 = histograms[(value + d + 1)
                                * histogram_width + histogram_col];
      uint16_t p_2 = histograms[(value - d - 1)
                                * histogram_width + histogram_col];
      if (p_1 > p_2) {
        val = (float)(diff_weights[d] * p_0 * p_1) / (float)(p_1 + p_0);
      } else {
        val = (float)(diff_weights[d] * p_0 * p_2) / (float)(p_2 + p_0);
      }

      if (val > c_value) {
        c_value = val;
      }
    }
  }
  return c_value;
}

static inline void calculate_c_values_row(float *c_values, uint16_t *histograms,
                                          uint16_t *image, const uint16_t *mask,
                                          int row, int width, ptrdiff_t stride,
                                          const uint16_t num_diffs,
                                          const uint16_t *tvi_for_diff,
                                          BandDetectInfo *dbi) {
  for (int col = 0; col < width; col++) {
    if (mask[row * stride + col]) {
      c_values[row * width + col] = c_value_pixel(
          histograms, image[row * stride + col] + num_diffs,
          dbi->diffs_weights, num_diffs, tvi_for_diff, col, width);
    }
  }
}

static void calculate_c_values(BandDetectInfo *dbi, int width, int height) {
  uint16_t *image = dbi->frame;
  uint16_t *mask = dbi->mask;
  ptrdiff_t stride = dbi->stride;
  float *c_values = dbi->buffers.c_values;
  uint16_t *histograms = dbi->buffers.c_values_histograms;
  uint16_t window_size = dbi->window_size;
  uint16_t num_diffs = dbi->num_diffs;
  uint16_t *tvi_for_diff = dbi->tvi_for_diff;
  uint16_t pad_size = window_size >> 1;
  const uint16_t num_bins = 1024 + 2*num_diffs;

  memset(c_values, 0.0, sizeof(float) * width * height);

  // Use a histogram for each pixel in width
  // histograms[i * width + j] accesses the j'th histogram, i'th value
  // This is done for cache optimization reasons
  memset(histograms, 0, width * num_bins * sizeof(uint16_t));

  // First pass: first pad_size rows
  for (int i = 0; i < pad_size; i++) {
    for (int j = 0; j < pad_size; j++) {
      cambi_histogram_add_edge_first_pass(histograms, image, mask, i, j,
                                          width, stride, pad_size, num_diffs);
    }
    for (int j = pad_size; j < width - pad_size - 1; j++) {
      cambi_histogram_add_first_pass(histograms, image, mask, i, j, width,
                                     stride, pad_size, num_diffs);
    }
    for (int j = AVMMAX(width - pad_size - 1, pad_size); j < width; j++) {
      cambi_histogram_add_edge_first_pass(histograms, image, mask, i, j,
                                          width, stride, pad_size, num_diffs);
    }
  }

  // Iterate over all rows, unrolled into 3 loops to avoid conditions
  for (int i = 0; i < pad_size + 1; i++) {
    if (i + pad_size < height) {
      for (int j = 0; j < pad_size; j++) {
        cambi_histogram_add_edge(histograms, image, mask, i, j, width, stride,
                                 pad_size, num_diffs);
      }
      for (int j = pad_size; j < width - pad_size - 1; j++) {
        cambi_histogram_add(histograms, image, mask, i, j, width, stride,
                            pad_size, num_diffs);
      }
      for (int j = AVMMAX(width - pad_size - 1, pad_size); j < width; j++) {
        cambi_histogram_add_edge(histograms, image, mask, i, j, width, stride,
                                 pad_size, num_diffs);
      }
    }
    calculate_c_values_row(c_values, histograms, image, mask, i, width, stride,
                           num_diffs, tvi_for_diff, dbi);
  }

  for (int i = pad_size + 1; i < height - pad_size; i++) {
    for (int j = 0; j < pad_size; j++) {
      cambi_histogram_sub_edge(histograms, image, mask, i, j, width, stride,
                               pad_size, num_diffs);
      cambi_histogram_add_edge(histograms, image, mask, i, j, width, stride,
                               pad_size, num_diffs);
    }
    for (int j = pad_size; j < width - pad_size - 1; j++) {
      cambi_histogram_sub(histograms, image, mask, i, j, width, stride,
                          pad_size, num_diffs);
      cambi_histogram_add(histograms, image, mask, i, j, width, stride,
                          pad_size, num_diffs);
    }
    for (int j = AVMMAX(width - pad_size - 1, pad_size); j < width; j++) {
      cambi_histogram_sub_edge(histograms, image, mask, i, j, width, stride,
                               pad_size, num_diffs);
      cambi_histogram_add_edge(histograms, image, mask, i, j, width, stride,
                               pad_size, num_diffs);
    }
    calculate_c_values_row(c_values, histograms, image, mask, i, width, stride,
                           num_diffs, tvi_for_diff, dbi);
  }

  for (int i = height - pad_size; i < height; i++) {
    if (i - pad_size - 1 >= 0) {
      for (int j = 0; j < pad_size; j++) {
        cambi_histogram_sub_edge(histograms, image, mask, i, j, width, stride,
                                 pad_size, num_diffs);
      }
      for (int j = pad_size; j < width - pad_size - 1; j++) {
        cambi_histogram_sub(histograms, image, mask, i, j, width, stride,
                            pad_size, num_diffs);
      }
      for (int j = AVMMAX(width - pad_size - 1, pad_size); j < width; j++) {
        cambi_histogram_sub_edge(histograms, image, mask, i, j, width, stride,
                                 pad_size, num_diffs);
      }
    }
    calculate_c_values_row(c_values, histograms, image, mask, i, width, stride,
                           num_diffs, tvi_for_diff, dbi);
  }
}

static double average_topk_elements(const float *arr, int topk_elements) {
  double sum = 0;
  for (int i = 0; i < topk_elements; i++)
    sum += arr[i];

  return (double) sum / topk_elements;
}

static void quick_select(float *arr, int n, int k) {
  if (n == k) return;
  int left = 0;
  int right = n - 1;
  while (left < right) {
    float pivot = arr[k];
    int i = left;
    int j = right;
    do {
      while (arr[i] > pivot) {
        i++;
      }
      while (arr[j] < pivot) {
        j--;
      }
      if (i <= j) {
        SWAP_FLOATS(arr[i], arr[j]);
        i++;
        j--;
      }
    } while (i <= j);
    if (j < k) {
      left = i;
    }
    if (k < i) {
      right = j;
    }
  }
}

static double spatial_pooling(float *c_values, double topk,
                              int width, int height) {
  int num_elements = height * width;
  int topk_num_elements = CLAMP((int) (topk * num_elements), 1, num_elements);
  quick_select(c_values, num_elements, topk_num_elements);
  return average_topk_elements(c_values, topk_num_elements);
}

// Inner product weighting scores for each scale
static inline double weight_scores_per_scale(const int *scale_weights,
                                             const double *scores_per_scale,
                                             uint16_t normalization) {
  double score = 0.0;
  for (unsigned scale = 0; scale < CAMBI_NUM_SCALES; scale++) {
    score += scores_per_scale[scale] * scale_weights[scale];
  }
  return score / normalization;
}

double cambi_score(BandDetectInfo *dbi, int frame_width, int frame_height) {
  double scores_per_scale[CAMBI_NUM_SCALES];
  int scaled_width = frame_width;
  int scaled_height = frame_height;

  for (unsigned scale = 0; scale < CAMBI_NUM_SCALES; scale++) {
    if (scale > 0) {
      scaled_width = (scaled_width + 1) >> 1;
      scaled_height = (scaled_height + 1) >> 1;
      cambi_decimate(dbi->frame, dbi->stride, scaled_width, scaled_height);
      cambi_decimate(dbi->mask, dbi->stride, scaled_width, scaled_height);
    }
    else {
      cambi_get_spatial_mask(dbi, scaled_width, scaled_height);
    }
    cambi_filter_mode(dbi, scaled_width, scaled_height);
    calculate_c_values(dbi, scaled_width, scaled_height);
    scores_per_scale[scale] = spatial_pooling(dbi->buffers.c_values, dbi->topk,
                                              scaled_width, scaled_height);
  }

  return weight_scores_per_scale(dbi->scale_weights, scores_per_scale,
                                 dbi->pixels_in_window);
}

double avm_compute_cambi(const YV12_BUFFER_CONFIG *frame, BandDetectInfo *dbi, MACROBLOCKD *xd) {
  av2_setup_dst_planes(xd->plane, frame, 0, 0, 0, 1, NULL);
  struct buf_2d pre_buf = xd->plane[0].dst;
  int src_stride = xd->plane[0].dst.stride;
  int frame_width = pre_buf.width;
  int frame_height = pre_buf.height;
  int bit_depth = xd->bd;
  uint16_t *src16 = pre_buf.buf;

  set_cambi_window(dbi);
  cambi_preprocessing(src16, src_stride, frame_width, frame_height, bit_depth,
                      dbi->frame, dbi->stride, frame_width, frame_height);
  return cambi_score(dbi, frame_width, frame_height);
}


// TODO(Joel): move CAMBI initialization & buffers
static const int cambi_scale_weights[CAMBI_NUM_SCALES] = {16, 8, 4, 2, 1};
static const int cambi_contrast_weights[8] = {1, 2, 3, 4, 4, 5, 5, 6};

void set_contrast_arrays_cambi(BandDetectInfo *const dbi) {
  int num_diffs = dbi->num_diffs;

  dbi->diffs_weights = avm_malloc(sizeof(dbi->diffs_weights) * num_diffs);
  for (int d = 0; d < num_diffs; d++) {
    dbi->diffs_weights[d] = cambi_contrast_weights[d];
  }

  for (int scale = 0; scale < CAMBI_NUM_SCALES; scale++) {
      dbi->scale_weights[scale] = cambi_scale_weights[scale];
  }
}

void set_tvi_per_contrast(BandDetectInfo *const dbi, int bitdepth) {
  (void) bitdepth;
  int num_diffs = dbi->num_diffs;
  dbi->tvi_for_diff = avm_malloc(sizeof(dbi->tvi_for_diff) * num_diffs);
  dbi->tvi_threshold = CAMBI_TVI;

  // Todo(Joel): Set TVI values from encoder input parameters
  for (int d=0; d<num_diffs; d++) {
    dbi->tvi_for_diff[d] = -1;
  }
}


/* CAMBI initialization */
int avm_band_detection_init(BandDetectInfo *const dbi, const int frame_width,
                            const int frame_height, const int bit_depth) {
  if (frame_width < CAMBI_MIN_WIDTH || frame_height > CAMBI_MAX_WIDTH ||
      !(bit_depth==8 || bit_depth==10)) {
    dbi->band_detected = 0;
    dbi->do_band_detection = 0;
    return dbi->do_band_detection;
  }

  dbi->do_band_detection = 1;
  dbi->topk = CAMBI_DEFAULT_TOPK_POOLING;
  dbi->stride = frame_width;
  dbi->height = frame_height;
  dbi->frame = avm_malloc(sizeof(*dbi->frame) * frame_height
                          * dbi->stride);

  dbi->max_log_contrast = CAMBI_DEFAULT_MAX_LOG_CONTRAST;
  dbi->num_diffs = 1 << dbi->max_log_contrast;
  set_contrast_arrays_cambi(dbi);
  set_tvi_per_contrast(dbi, 10);
  dbi->num_bins = (1 << 10) + 2 * dbi->num_diffs;

  dbi->buffers.filter_mode_buffer = avm_malloc(3 * frame_width
                                               * sizeof(uint16_t));

  int pad_size = CAMBI_MASK_FILTER_SIZE >> 1;
  int dp_width = frame_width + 2 * pad_size + 1;
  int dp_height = 2 * pad_size + 2;
  dbi->buffers.mask_dp = avm_malloc(dp_height * dp_width * sizeof(uint32_t));

  dbi->mask = avm_malloc(sizeof(*dbi->mask) * frame_height
                         * dbi->stride);
  dbi->buffers.c_values = avm_malloc(sizeof(float) * frame_height
                                     * dbi->stride);
  dbi->buffers.c_values_histograms =
      avm_malloc(frame_width * dbi->num_bins * sizeof(uint16_t));

  return dbi->do_band_detection;
}

void avm_band_detection_close(BandDetectInfo *const dbi) {
  if (!dbi->do_band_detection) {
    return;
  }
  avm_free(dbi->frame);
  avm_free(dbi->mask);

  // set_tvi_per_contrast
  avm_free(dbi->tvi_for_diff);

  avm_free(dbi->buffers.filter_mode_buffer);
  avm_free(dbi->buffers.mask_dp);

  avm_free(dbi->diffs_weights);
  avm_free(dbi->buffers.c_values);
  avm_free(dbi->buffers.c_values_histograms);
}

/*!\brief Assess banding via CAMBI
 *
 * Searches for presence of banding computing CAMBI on refernce and distorted
 *
 * \param[in]      frame        Compressed frame buffer
 * \param[in]      ref          Source frame buffer
 * \param[in,out]  cm           Pointer to top level common structure
 * \param[in]      xd           Pointer to common current coding block structure
 * \param[out]     band_metadata   Banding hints metadata structure to be filled

 *
 * \return Nothing is returned. Instead, presence of banding is stored
 */
void avm_band_detection(const YV12_BUFFER_CONFIG *frame,
                        const YV12_BUFFER_CONFIG *ref, AV2_COMMON *cm,
                        MACROBLOCKD *xd,
                        avm_banding_hints_metadata_t *band_metadata) {
  BandDetectInfo *const dbi = &cm->band_info;
  double cambi_ref = avm_compute_cambi(ref, dbi, xd);
  double cambi_enc = avm_compute_cambi(frame, dbi, xd);

  int bit_depth = xd->bd;
  double diff_threshold = bit_depth==8 ? CAMBI_DIFF_THRESHOLD_8b
                                       : CAMBI_DIFF_THRESHOLD_10b;

  dbi->band_detected = (cambi_enc - cambi_ref >= diff_threshold);

  double src_threshold = bit_depth==8 ? CAMBI_SOURCE_THRESHOLD_8b
                                      : CAMBI_SOURCE_THRESHOLD_10b;

  // Initialize and populate the band metadata structure
  if (band_metadata) {
    memset(band_metadata, 0, sizeof(avm_banding_hints_metadata_t));
    band_metadata->coding_banding_present_flag = dbi->band_detected;
    band_metadata->source_banding_present_flag = (cambi_ref < src_threshold);
    // For now, set banding_hints_flag to 0 (no detailed hints)
    // This can be extended in the future to include more detailed information
    band_metadata->banding_hints_flag = 0;
  }

  printf("CAMBI ref: %f  CAMBI enc: %f, detection: %d\n",
         cambi_ref, cambi_enc, dbi->band_detected);
}
