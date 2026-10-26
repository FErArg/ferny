// Synthetic fixture: more output than the Compiler tab keeps (H2.4).
#include <stdio.h>

int main(void) {
    for (int i = 0; i < 20000; i++) {
        printf("line %d\n", i);
    }
    return 0;
}
