/*
 * What this build of Dropbear is: the ssh client alone, with its keys
 * tool. No server (an app cannot listen as a login service), and none of
 * what starts another program (a proxy command, the agent), since only
 * one program runs at a time here.
 */
#define DROPBEAR_SERVER 0
#define DROPBEAR_CLIENT 1
#define DROPBEAR_CLI_PROXYCMD 0
#define DROPBEAR_CLI_NETCAT 0
#define DROPBEAR_CLI_AGENTFWD 0
#define DROPBEAR_CLI_COMPRESSION 1
#define DO_HOST_LOOKUP 1
#define DROPBEAR_USE_PASSWORD_ENV 0
/* The server's options, all off with it. */
#define DROPBEAR_SVR_PASSWORD_AUTH 0
/* Left on: sysoptions.h wants one server auth type, and no server code is built. */
#define DROPBEAR_SVR_PUBKEY_AUTH 1
#define DROPBEAR_SVR_DROP_PRIVS 0
#define DROPBEAR_SVR_AGENTFWD 0
#define DROPBEAR_SVR_LOCALTCPFWD 0
#define DROPBEAR_SVR_REMOTETCPFWD 0
#define DROPBEAR_SVR_LOCALSTREAMFWD 0
#define DROPBEAR_X11FWD 0
#define DROPBEAR_PLUGIN 0
