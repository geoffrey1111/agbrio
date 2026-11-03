#include <sys/param.h>
#include <libproc.h>
#include <sys/proc_info.h>
#include <sys/sysctl.h>
#include <sys/stat.h>
#include <sys/file.h>
#include <unistd.h>
enum { AGBRIO_RESOURCE_PATH_CAPACITY = PROC_PIDPATHINFO_MAXSIZE };
