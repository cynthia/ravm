#include "tools/ml/py_bridge.h"

#include <algorithm>
#include <iterator>
#include <cstdlib>
#include "cnpy.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#include <fnmatch.h> // For pattern matching
#include <unistd.h>   // For unlink (file deletion)
#include <filesystem>
#include <sys/stat.h>

struct Var {
  std::vector<size_t> shape;
  std::vector<double> data;
};

struct DataFile {
  std::map<std::string, Var> vars;
};

struct PyDataFile {
  std::map<std::string, DataFile> files;
};

extern "C" void* py_datafile_open() {
  struct PyDataFile* df = new struct PyDataFile();
  return (void*)df;
}

static size_t get_data_length(std::vector<size_t> shape) {
  int data_len = 1;
  for (size_t v : shape) {
    data_len *= v;
  }
  return data_len;
}

static void py_datafile_add_int(void** context, const char* file,
                                const char* var_name, float* var_data,
                                std::vector<size_t> shape) {
  struct PyDataFile* df = (struct PyDataFile*)*context;
  if (df == nullptr) {
    printf("Creating py datafile\n");
    df = new struct PyDataFile();
    *context = (void*)df;
  }
  if (!df->files[file].vars.count(var_name)) {
    df->files[file].vars[var_name].shape = shape;
  } else {
    const auto& old_shape = df->files[file].vars[var_name].shape;
    if (old_shape != shape) {
      std::cout << "Illegal shape for var:" << var_name << ", was ";
      std::copy(old_shape.begin(), old_shape.end(), std::ostream_iterator<int>(std::cout, ", "));
      std::cout << " now ";
      std::copy(shape.begin(), shape.end(), std::ostream_iterator<int>(std::cout, ", "));
      std::cout << std::endl;
      std::exit(1);
    }
  }
  auto& data = df->files[file].vars[var_name].data;
  for (size_t i = 0; i < get_data_length(shape); i++) {
    data.push_back(var_data[i]);
  }
}

extern "C" void py_datafile_add_0d(void** context, const char* file,
                                const char* var_name, float* var_data) {
  py_datafile_add_int(context, file, var_name, var_data, {});
}

extern "C" void py_datafile_add_1d(void** context, const char* file,
                                const char* var_name, float* var_data,
                                size_t shape0) {
  py_datafile_add_int(context, file, var_name, var_data, {shape0});
}

extern "C" void py_datafile_add_2d(void** context, const char* file,
                                const char* var_name, float* var_data,
                                size_t shape0, size_t shape1) {
  py_datafile_add_int(context, file, var_name, var_data, {shape0, shape1});
}

void py_datafile_fold(void** _to, void** _from) {
  struct PyDataFile* to = (struct PyDataFile*)*_to;
  struct PyDataFile* from = (struct PyDataFile*)*_from;
  if (from == nullptr) {
    std::cout << "\x1b[91mpy_data_fold: from is null\x1b[0m" << std::endl;
    return;
  }
  if (to == nullptr) {
    to = new struct PyDataFile();
    *_to = (void*)to;
  }
  for (const auto& e0 : from->files) {
    for (const auto& e1 : e0.second.vars) {
      const auto& from_var = e1.second;
      if (!to->files[e0.first].vars.count(e1.first)) {
        to->files[e0.first].vars[e1.first].shape = from_var.shape;
      } else {
        const auto& to_shape = to->files[e0.first].vars[e1.first].shape;
        if (to_shape != from_var.shape) {
          std::cout << "Illegal shape for var:" << e1.first << ", was ";
          std::copy(to_shape.begin(), to_shape.end(), std::ostream_iterator<int>(std::cout, ", "));
          std::cout << " now ";
          std::copy(from_var.shape.begin(), from_var.shape.end(), std::ostream_iterator<int>(std::cout, ", "));
          std::cout << std::endl;
          std::exit(1);
        }
      }
      auto& to_var = to->files[e0.first].vars[e1.first];
      to_var.data.insert(to_var.data.end(), from_var.data.begin(), from_var.data.end());
    }
  }
  delete from;
  *_from = nullptr;
}

extern "C" int py_datafile_delete_old_files(const char* _filename, size_t* count) {
  DIR *dir;
  struct dirent *entry;
  char filepath[256];
  char pattern[128];
  const char* dirname;
  const char* basename;

  *count = 0;
  char path_copy[strlen(_filename) + 1];
  strcpy(path_copy, _filename);
  char *last_slash = strrchr(path_copy, '/');
  if (last_slash !=  NULL) {
    *last_slash = '\0';
    dirname = path_copy;
    basename = last_slash + 1;
  } else {
    dirname = ".";
    basename = path_copy;
  }

  if ((dir = opendir(dirname)) == NULL) {
    return 1;
  }

  sprintf(pattern, "%s*.npz", basename);

  int ret = 0;
  while ((entry = readdir(dir))!= NULL) {
    if (fnmatch(pattern, entry->d_name, 0) == 0) {
      snprintf(filepath, sizeof(filepath), "%s/%s", dirname, entry->d_name);
      if (unlink(filepath)) {
        ret = 1;
      } else {
        (*count)++;
      }
    }
  }
  closedir(dir);
  return ret;
}

bool dir_exists(const std::string& file_path) {
  std::filesystem::path path = std::filesystem::path(file_path).parent_path();
  if (path.empty())
    return true;
  struct stat info;
  if (stat(path.c_str(), &info) != 0) {
    return false; // Path does not exist or is not accessible
  }
  return (info.st_mode & S_IFDIR) != 0; // Check if it's a directory
}

extern "C" void py_datafile_close(void** context, const char* _filename) {
  struct PyDataFile* df = (struct PyDataFile*)*context;
  if (df == nullptr) {
    std::cout << "\x1b[91mpy_data_close: df is null\x1b[0m" << std::endl;
    return;
  }
  if (!dir_exists(std::string(_filename))) {
    std::cout << "\x1b[91mpy_data_close: tring to create a file "
              << std::string(_filename) << " in a directory that doesn't "
              << "exist.\x1b[0m" << std::endl;
    return;
  }
  for (const auto& e0 : df->files) {
    const auto filename = std::string(_filename) + "_" + e0.first + ".npz";
    for (const auto& e1 : e0.second.vars) {
      const auto& var = e1.second;
      const unsigned int data_length = get_data_length(var.shape);
      std::vector<size_t> npz_shape;
      npz_shape.push_back(var.data.size() / data_length);
      npz_shape.insert(npz_shape.end(), var.shape.begin(), var.shape.end());
      cnpy::npz_save(filename, e1.first, &var.data[0], npz_shape, "a");
    }
  }
  delete df;
  *context = nullptr;
}

