#ifndef _MATH_H
#define _MATH_H
#define M_E 2.71828182845904523536
#define M_LOG2E 1.44269504088896340736
#define M_LOG10E 0.434294481903251827651
#define M_LN2 0.693147180559945309417
#define M_LN10 2.30258509299404568402
#define M_PI 3.14159265358979323846
#define M_PI_2 1.57079632679489661923
#define M_PI_4 0.785398163397448309616
#define M_1_PI 0.318309886183790671538
#define M_2_PI 0.636619772367581343076
#define M_SQRT2 1.41421356237309504880
#define M_SQRT1_2 0.707106781186547524401
#define HUGE_VAL (1.0 / 0.0)
#define INFINITY (1.0f / 0.0f)
#define NAN (0.0f / 0.0f)
#define isnan(x) ((x) != (x))
#define isinf(x) ((x) == (1.0 / 0.0) || (x) == -(1.0 / 0.0))
#define isfinite(x) (!isnan(x) && !isinf(x))
double sin(double); double cos(double); double tan(double);
double asin(double); double acos(double); double atan(double); double atan2(double, double);
double sinh(double); double cosh(double); double tanh(double);
double exp(double); double exp2(double); double expm1(double);
double log(double); double log2(double); double log10(double); double log1p(double);
double pow(double, double); double sqrt(double); double cbrt(double); double hypot(double, double);
double ceil(double); double floor(double); double round(double); double trunc(double); double rint(double);
double fabs(double); double fmod(double, double); double remainder(double, double);
double fmin(double, double); double fmax(double, double); double copysign(double, double);
double ldexp(double, int); double frexp(double, int *); double modf(double, double *);
long lround(double); long long llround(double);
float sinf(float); float cosf(float); float tanf(float); float sqrtf(float); float powf(float, float);
float floorf(float); float ceilf(float); float fabsf(float); float expf(float); float logf(float);
#endif
