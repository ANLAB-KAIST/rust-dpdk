#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <sys/types.h>
#include <inttypes.h>
#include "dpdk.h"

#define U32_FMT "%" PRIu32
#define U64_FMT "%" PRIu64
#define I32_FMT "%" PRId32
#define I64_FMT "%" PRId64

int main() {
    printf(__CHECK_FMT "\n", __CHECK_VAL);
    return 0;
}
