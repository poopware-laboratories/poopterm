#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
int main(int argc, char **argv) {
	char *text = strdup("poopterm");
	printf("hello from tcc: %s, fib(20)=%d, argc=%d, pi=%.3f, big=%ld\n", text, fib(20), argc, 3.14159, 1L << 40);
	free(text);
	return 7;
}
