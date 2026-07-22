#include <lean/lean.h>

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static lean_obj_res aw_io_error(const char *operation) {
  int code = errno;
  char message[512];
  snprintf(message, sizeof(message), "%s: %s", operation, strerror(code));
  return lean_io_result_mk_error(
      lean_mk_io_error_other_error(code, lean_mk_string(message)));
}

static int aw_write_all(int fd, const uint8_t *bytes, size_t size) {
  size_t written = 0;
  while (written < size) {
    ssize_t amount = write(fd, bytes + written, size - written);
    if (amount < 0) {
      if (errno == EINTR) continue;
      return -1;
    }
    written += (size_t)amount;
  }
  return 0;
}

static int aw_existing_matches(int fd, const uint8_t *bytes, size_t size) {
  struct stat info;
  if (fstat(fd, &info) != 0 || info.st_size < 0 || (size_t)info.st_size != size)
    return 0;
  size_t read_count = 0;
  uint8_t buffer[8192];
  while (read_count < size) {
    size_t wanted = size - read_count;
    if (wanted > sizeof(buffer)) wanted = sizeof(buffer);
    ssize_t amount = pread(fd, buffer, wanted, (off_t)read_count);
    if (amount < 0) {
      if (errno == EINTR) continue;
      return 0;
    }
    if (amount == 0 || memcmp(buffer, bytes + read_count, (size_t)amount) != 0)
      return 0;
    read_count += (size_t)amount;
  }
  return 1;
}

static int aw_fsync_parent(const char *path) {
  char *copy = strdup(path);
  if (copy == NULL) return -1;
  char *separator = strrchr(copy, '/');
  if (separator == NULL) {
    free(copy);
    errno = EINVAL;
    return -1;
  }
  *separator = '\0';
  int directory = open(copy, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
  free(copy);
  if (directory < 0) return -1;
  int result = fsync(directory);
  int saved = errno;
  close(directory);
  errno = saved;
  return result;
}

LEAN_EXPORT lean_obj_res aw_stage_durable_file(
    b_lean_obj_arg temp_obj, b_lean_obj_arg final_obj, b_lean_obj_arg bytes_obj) {
  const char *temp = lean_string_cstr(temp_obj);
  const char *final = lean_string_cstr(final_obj);
  const uint8_t *bytes = lean_sarray_cptr(bytes_obj);
  size_t size = lean_sarray_size(bytes_obj);

  int fd = open(temp, O_RDWR | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
  if (fd < 0 && errno == EEXIST) {
    fd = open(temp, O_RDWR | O_CLOEXEC);
    if (fd < 0) return aw_io_error("open staged artifact");
    if (!aw_existing_matches(fd, bytes, size)) {
      close(fd);
      errno = EEXIST;
      return aw_io_error("staged artifact payload conflict");
    }
  } else if (fd < 0) {
    return aw_io_error("create staged artifact");
  } else if (aw_write_all(fd, bytes, size) != 0) {
    int saved = errno;
    close(fd);
    unlink(temp);
    errno = saved;
    return aw_io_error("write staged artifact");
  }

  if (fsync(fd) != 0) {
    int saved = errno;
    close(fd);
    errno = saved;
    return aw_io_error("flush staged artifact");
  }
  if (close(fd) != 0) return aw_io_error("close staged artifact");

  uint32_t existed = 0;
  if (link(temp, final) != 0) {
    if (errno != EEXIST) return aw_io_error("adopt staged artifact");
    existed = 1;
  }
  if (unlink(temp) != 0) return aw_io_error("remove staged artifact");
  if (aw_fsync_parent(final) != 0) return aw_io_error("flush artifact directory");
  return lean_io_result_mk_ok(lean_box_uint32(existed));
}

LEAN_EXPORT lean_obj_res aw_replace_durable_file(
    b_lean_obj_arg staged_obj, b_lean_obj_arg current_obj) {
  const char *staged = lean_string_cstr(staged_obj);
  const char *current = lean_string_cstr(current_obj);
  int fd = open(staged, O_RDONLY | O_CLOEXEC);
  if (fd < 0) return aw_io_error("open replacement");
  if (fsync(fd) != 0) {
    int saved = errno;
    close(fd);
    errno = saved;
    return aw_io_error("flush replacement");
  }
  if (close(fd) != 0) return aw_io_error("close replacement");
  if (rename(staged, current) != 0) return aw_io_error("publish replacement");
  if (aw_fsync_parent(current) != 0) return aw_io_error("flush replacement directory");
  return lean_io_result_mk_ok(lean_box(0));
}
