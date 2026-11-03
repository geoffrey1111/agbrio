#include <sys/param.h>
#include <libproc.h>
#include <sys/proc_info.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>
// Swift cannot import this SDK macro's expression directly.
enum { AGBRIO_PROC_PATH_MAXSIZE = PROC_PIDPATHINFO_MAXSIZE };
