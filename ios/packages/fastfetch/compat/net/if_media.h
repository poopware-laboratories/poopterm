/*
 * What fastfetch's interface detection uses of <net/if_media.h>, which
 * the iOS SDK leaves out: the macros that split the media word a
 * SIOCGIFMEDIA request answers (type in bits 5-7, subtype in 0-4), as
 * if_media(4) documents them; struct ifmediareq is in <net/if.h>. Only the Ethernet type is named, as only
 * it is asked about.
 */
#ifndef POOPTERM_NET_IF_MEDIA_H
#define POOPTERM_NET_IF_MEDIA_H

#include <net/if.h>

#define IFM_NMASK 0x000000e0
#define IFM_TMASK 0x0000001f
#define IFM_ETHER 0x00000020
#define IFM_TYPE(x) ((x) & IFM_NMASK)
#define IFM_SUBTYPE(x) ((x) & IFM_TMASK)

#endif
