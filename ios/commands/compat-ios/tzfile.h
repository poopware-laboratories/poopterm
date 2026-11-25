/*
 * <tzfile.h>, missing from the iOS SDK: the calendar constants pax uses
 * to read dates.
 */
#ifndef POOPTERM_TZFILE_H
#define POOPTERM_TZFILE_H
#define SECSPERMIN 60
#define MINSPERHOUR 60
#define HOURSPERDAY 24
#define DAYSPERWEEK 7
#define DAYSPERNYEAR 365
#define DAYSPERLYEAR 366
#define SECSPERHOUR (SECSPERMIN * MINSPERHOUR)
#define SECSPERDAY ((long)SECSPERHOUR * HOURSPERDAY)
#define MONSPERYEAR 12
#define TM_YEAR_BASE 1900
#define EPOCH_YEAR 1970
#define isleap(y) (((y) % 4) == 0 && (((y) % 100) != 0 || ((y) % 400) == 0))
#endif
