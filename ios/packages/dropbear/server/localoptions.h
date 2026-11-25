/*
 * What this build of Dropbear is: the ssh server alone, serving one
 * connection a call from the app, with sessions that run in the app's own
 * shell (no fork, no pty, no other user). No forwarding, agent or X11.
 */
#define DROPBEAR_SERVER 1
#define DROPBEAR_CLIENT 0
#define DROPBEAR_CLI_PROXYCMD 0
#define DROPBEAR_CLI_NETCAT 0
#define DROPBEAR_CLI_AGENTFWD 0
#define DROPBEAR_USE_PASSWORD_ENV 0
#define DROPBEAR_SVR_PASSWORD_AUTH 1
#define DROPBEAR_SVR_PUBKEY_AUTH 1
#define DROPBEAR_SVR_DROP_PRIVS 0
#define DROPBEAR_SVR_MULTIUSER 1
#define DROPBEAR_SVR_AGENTFWD 0
#define DROPBEAR_SVR_LOCALTCPFWD 0
#define DROPBEAR_SVR_REMOTETCPFWD 0
#define DROPBEAR_SVR_LOCALSTREAMFWD 0
#define DROPBEAR_X11FWD 0
#define DROPBEAR_PLUGIN 0
#define DROPBEAR_SFTPSERVER 0
#define DO_MOTD 0
#define INETD_MODE 1
#define NON_INETD_MODE 0
#define DROPBEAR_DO_REEXEC 0
#define DO_HOST_LOOKUP 0
