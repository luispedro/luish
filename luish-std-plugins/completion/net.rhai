// Specs for network and crypto tools: curl, wget, ip, ss, ping, dig,
// ssh-add, ssh-keygen, ssh-copy-id, gpg and openssl (see lib.rhai for their
// fields).

import "kinds" as kinds;

// The network interfaces.
fn interfaces() {
    fs::list_dir("/sys/class/net") ?? []
}

// The network namespaces.
fn netns() {
    fs::list_dir("/run/netns") ?? []
}

// ip's arguments: keywords, interfaces after `dev` (and some others), and
// namespaces after `netns`.
fn ip_args(words) {
    let prev = words[-2];
    if prev in ["dev", "master", "link", "iif", "oif", "vrf"] {
        return interfaces();
    }
    if prev == "netns" {
        return netns();
    }
    let out = [
        "dev", "up", "down", "to", "via", "table", "proto", "scope", "label", "type", "name", "mtu", "master",
        "nomaster", "address", "broadcast", "peer", "metric", "src", "default", "netns", "vrf",
    ];
    out + interfaces()
}

// The record types of DNS.
fn record_types() {
    [
        "A", "AAAA", "ANY", "AXFR", "CAA", "CNAME", "DNSKEY", "DS", "HINFO", "HTTPS", "IXFR", "MX", "NAPTR", "NS",
        "NSEC", "NSEC3", "PTR", "RRSIG", "SOA", "SPF", "SRV", "SSHFP", "SVCB", "TLSA", "TXT",
    ]
}

// dig's arguments: `@SERVER`, `+OPTION`, and names and record types.
fn dig_args(cur) {
    if cur.starts_with("@") {
        return #{prefix: "@", candidates: kinds::host_names()};
    }
    if cur.starts_with("+") {
        let opts = [
            "aaflag", "aaonly", "additional", "adflag", "all", "answer", "authority", "besteffort", "bufsize=",
            "cdflag", "class", "cmd", "comments", "cookie", "crypto", "defname", "dnssec", "domain=", "edns",
            "expire", "fail", "identify", "ignore", "keepalive", "keepopen", "multiline", "ndots=", "nsid",
            "nssearch", "onesoa", "qr", "question", "recurse", "retry=", "rrcomments", "search", "short",
            "showsearch", "split=", "stats", "subnet=", "tcp", "timeout=", "tls", "trace", "tries=", "ttlid",
            "ttlunits", "vc", "yaml",
        ];
        let out = [];
        for o in opts {
            out.push(if o.ends_with("=") { #{value: o, suffix: ""} } else { o });
            if !o.ends_with("=") {
                out.push("no" + o);
            }
        }
        return #{prefix: "+", candidates: out};
    }
    kinds::host_names() + record_types()
}

// The e-mail addresses of the keys in the keyring (the secret ones for
// `secret`), with their names.
fn gpg_keys(secret) {
    let r = sh::capture(["gpg", "--list-" + (if secret { "secret-" } else { "" }) + "keys", "--with-colons"]);
    let out = [];
    for line in r.out.split("\n") {
        if !line.starts_with("uid:") {
            continue;
        }
        let f = line.split(":");
        if f.len() < 10 {
            continue;
        }
        let uid = f[9];
        let lt = uid.index_of("<");
        let gt = uid.index_of(">");
        if lt >= 0 && gt > lt {
            let name = uid.sub_string(0, lt);
            name.trim();
            out.push(#{value: uid.sub_string(lt + 1, gt - lt - 1), desc: name});
        }
    }
    out
}

// The options of an openssl command, from its `-help`.
fn sub_spec(name, sub) {
    if name != "openssl" {
        throw `unknown sub_spec: ${name}`;
    }
    let r = sh::capture(["openssl", sub, "-help"], "merge");
    let opts = "";
    for line in r.out.split("\n") {
        if line.starts_with(" -") {
            opts += line + "\n";
        }
    }
    #{opts: opts, single_dash: true, guess_values: true}
}

fn kind(name, cur, words) {
    switch name {
        "interfaces" => interfaces(),
        "netns" => netns(),
        "ip_args" => ip_args(words),
        "dig_args" => dig_args(cur),
        "record_types" => record_types(),
        "keys" => gpg_keys(false),
        "secret_keys" => gpg_keys(true),
        _ => throw `unknown kind: ${name}`,
    }
}

fn curl() {
    #{
        opts: `
            --abstract-unix-socket <path>  connect via abstract Unix domain socket
            --alt-svc <file name>  enable alt-svc with this cache file
            --anyauth  pick any authentication method
            -a, --append  append to target file when uploading
            --aws-sigv4 <provider1[:provider2[:region[:service]]]>  use AWS V4 signature authentication
            --basic  use HTTP Basic Authentication
            --ca-native  use CA certificates from the native OS
            --cacert <file>  CA certificate to verify peer against
            --capath <dir>  CA directory to verify peer against
            -E, --cert <certificate[:password]>  client certificate file and password
            --cert-status  verify the status of the server cert via OCSP-staple
            --cert-type <type>  certificate type (DER/PEM/ENG/P12)
            --ciphers <list of ciphers>  SSL ciphers to use
            --compressed  request compressed response
            --compressed-ssh  enable SSH compression
            -K, --config <file>  read config from a file
            --connect-timeout <fractional seconds>  maximum time allowed for connection
            --connect-to <HOST1:PORT1:HOST2:PORT2>  connect to host
            -C, --continue-at <offset>  resumed transfer offset
            -b, --cookie <data|file>  send cookies from string/file
            -c, --cookie-jar <filename>  write cookies to <filename> after operation
            --create-dirs  create necessary local directory hierarchy
            --create-file-mode <mode>  file mode for created files
            --crlf  convert LF to CRLF in upload
            --crlfile <file>  use this CRL list
            --curves <algorithm list>  (EC) TLS key exchange algorithm(s) to request
            -d, --data <data>  HTTP POST data
            --data-ascii <data>  HTTP POST ASCII data
            --data-binary <data>  HTTP POST binary data
            --data-raw <data>  HTTP POST data, '@' allowed
            --data-urlencode <data>  HTTP POST data URL encoded
            --delegation <LEVEL>  GSS-API delegation permission
            --digest  use HTTP Digest Authentication
            -q, --disable  disable .curlrc
            --disable-eprt  inhibit using EPRT or LPRT
            --disable-epsv  inhibit using EPSV
            --disallow-username-in-url  disallow username in URL
            --dns-interface <interface>  interface to use for DNS requests
            --dns-ipv4-addr <address>  IPv4 address to use for DNS requests
            --dns-ipv6-addr <address>  IPv6 address to use for DNS requests
            --dns-servers <addresses>  DNS server addrs to use
            --doh-cert-status  verify the status of the DoH server cert via OCSP-staple
            --doh-insecure  allow insecure DoH server connections
            --doh-url <URL>  resolve host names over DoH
            -D, --dump-header <filename>  write the received headers to <filename>
            --egd-file <file>  EGD socket path for random data
            --engine <name>  crypto engine to use
            --etag-compare <file>  pass an ETag from a file as a custom header
            --etag-save <file>  parse ETag from a request and save it to a file
            --expect100-timeout <seconds>  how long to wait for 100-continue
            -f, --fail  fail fast with no output on HTTP errors
            --fail-early  fail on first transfer error, do not continue
            --fail-with-body  fail on HTTP errors but save the body
            --false-start  enable TLS False Start
            -F, --form <name=content>  specify multipart MIME data
            --form-escape  escape multipart form field/file names using backslash
            --form-string <name=string>  specify multipart MIME data
            --ftp-account <data>  account data string
            --ftp-alternative-to-user <command>  string to replace USER [name]
            --ftp-create-dirs  create the remote dirs if not present
            --ftp-method <method>  control CWD usage
            --ftp-pasv  use PASV/EPSV instead of PORT
            -P, --ftp-port <address>  use PORT instead of PASV
            --ftp-pret  send PRET before PASV
            --ftp-skip-pasv-ip  skip the IP address for PASV
            --ftp-ssl-ccc  send CCC after authenticating
            --ftp-ssl-ccc-mode <active/passive>  set CCC mode
            --ftp-ssl-control  require SSL/TLS for FTP login, clear for transfer
            -G, --get  put the post data in the URL and use GET
            -g, --globoff  disable URL sequences and ranges using {} and []
            --happy-eyeballs-timeout-ms <milliseconds>  time for IPv6 before trying IPv4
            --haproxy-clientip  sets client IP in HAProxy PROXY protocol v1 header
            --haproxy-protocol  send HAProxy PROXY protocol v1 header
            -I, --head  show document info only
            -H, --header <header/@file>  pass custom header(s) to server
            -h, --help <category>  get help for commands
            --hostpubmd5 <md5>  acceptable MD5 hash of the host public key
            --hostpubsha256 <sha256>  acceptable SHA256 hash of the host public key
            --hsts <file name>  enable HSTS with this cache file
            --http0.9  allow HTTP 0.9 responses
            -0, --http1.0  use HTTP 1.0
            --http1.1  use HTTP 1.1
            --http2  use HTTP/2
            --http2-prior-knowledge  use HTTP 2 without HTTP/1.1 Upgrade
            --http3  use HTTP v3
            --http3-only  use HTTP v3 only
            --ignore-content-length  ignore the size of the remote resource
            -i, --include  include protocol response headers in the output
            -k, --insecure  allow insecure server connections
            --interface <name>  use network INTERFACE (or address)
            --ipfs-gateway <URL>  gateway for IPFS
            -4, --ipv4  resolve names to IPv4 addresses
            -6, --ipv6  resolve names to IPv6 addresses
            --json <data>  HTTP POST JSON
            -j, --junk-session-cookies  ignore session cookies read from file
            --keepalive-time <seconds>  interval time for keepalive probes
            --key <key>  private key file name
            --key-type <type>  private key file type (DER/PEM/ENG)
            --krb <level>  enable Kerberos with security <level>
            --libcurl <file>  dump libcurl equivalent code of this command line
            --limit-rate <speed>  limit transfer speed to RATE
            -l, --list-only  list only mode
            --local-port <num/range>  force use of RANGE for local port numbers
            -L, --location  follow redirects
            --location-trusted  like --location, and send auth to other hosts
            --login-options <options>  server login options
            --mail-auth <address>  originator address of the original email
            --mail-from <address>  mail from this address
            --mail-rcpt <address>  mail to this address
            --mail-rcpt-allowfails  allow RCPT TO command to fail for some recipients
            -M, --manual  display the full manual
            --max-filesize <bytes>  maximum file size to download
            --max-redirs <num>  maximum number of redirects allowed
            -m, --max-time <fractional seconds>  maximum time allowed for transfer
            --metalink  process given URLs as metalink XML file
            --negotiate  use HTTP Negotiate (SPNEGO) authentication
            -n, --netrc  must read .netrc for user name and password
            --netrc-file <filename>  specify FILE for netrc
            --netrc-optional  use either .netrc or URL
            -:, --next  make next URL use its separate set of options
            --no-alpn  disable the ALPN TLS extension
            -N, --no-buffer  disable buffering of the output stream
            --no-clobber  do not overwrite files that already exist
            --no-keepalive  disable TCP keepalive on the connection
            --no-npn  disable the NPN TLS extension
            --no-progress-meter  do not show the progress meter
            --no-sessionid  disable SSL session-ID reusing
            --noproxy <no-proxy-list>  list of hosts which do not use proxy
            --ntlm  use HTTP NTLM authentication
            --ntlm-wb  use HTTP NTLM authentication with winbind
            --oauth2-bearer <token>  OAuth 2 Bearer Token
            -o, --output <file>  write to file instead of stdout
            --output-dir <dir>  directory to save files in
            -Z, --parallel  perform transfers in parallel
            --parallel-immediate  do not wait for multiplexing (with --parallel)
            --parallel-max <num>  maximum concurrency for parallel transfers
            --pass <phrase>  pass phrase for the private key
            --path-as-is  do not squash .. sequences in URL path
            --pinnedpubkey <hashes>  public key file or hashes to verify the peer against
            --post301  do not switch to GET after following a 301
            --post302  do not switch to GET after following a 302
            --post303  do not switch to GET after following a 303
            --preproxy <[protocol://]host[:port]>  use this proxy first
            -#, --progress-bar  display transfer progress as a bar
            --proto <protocols>  enable/disable PROTOCOLS
            --proto-default <protocol>  use PROTOCOL for any URL missing a scheme
            --proto-redir <protocols>  enable/disable PROTOCOLS on redirect
            -x, --proxy <[protocol://]host[:port]>  use this proxy
            --proxy-anyauth  pick any proxy authentication method
            --proxy-basic  use Basic authentication on the proxy
            --proxy-ca-native  use CA certificates from the native OS for proxy
            --proxy-cacert <file>  CA certificate to verify peer against for proxy
            --proxy-capath <dir>  CA directory to verify peer against for proxy
            --proxy-cert <cert[:passwd]>  set client certificate for proxy
            --proxy-cert-type <type>  client certificate type for HTTPS proxy
            --proxy-ciphers <list>  SSL ciphers to use for proxy
            --proxy-crlfile <file>  set a CRL list for proxy
            --proxy-digest  use Digest authentication on the proxy
            --proxy-header <header/@file>  pass custom header(s) to proxy
            --proxy-http2  use HTTP/2 with HTTPS proxy
            --proxy-insecure  do HTTPS proxy connections without verifying the proxy
            --proxy-key <key>  private key for HTTPS proxy
            --proxy-key-type <type>  private key file type for proxy
            --proxy-negotiate  use HTTP Negotiate (SPNEGO) authentication on the proxy
            --proxy-ntlm  use NTLM authentication on the proxy
            --proxy-pass <phrase>  pass phrase for the private key for HTTPS proxy
            --proxy-pinnedpubkey <hashes>  public key file or hashes to verify the proxy against
            --proxy-service-name <name>  SPNEGO proxy service name
            --proxy-ssl-allow-beast  allow security flaw for interop for HTTPS proxy
            --proxy-ssl-auto-client-cert  use auto client certificate for proxy (Schannel)
            --proxy-tls13-ciphers <ciphersuite list>  TLS 1.3 proxy cipher suites
            --proxy-tlsauthtype <type>  TLS authentication type for HTTPS proxy
            --proxy-tlspassword <string>  TLS password for HTTPS proxy
            --proxy-tlsuser <name>  TLS username for HTTPS proxy
            --proxy-tlsv1  use TLSv1 for HTTPS proxy
            -U, --proxy-user <user:password>  proxy user and password
            --proxy1.0 <host[:port]>  use HTTP/1.0 proxy on given port
            -p, --proxytunnel  operate through an HTTP proxy tunnel (using CONNECT)
            --pubkey <key>  SSH Public key file name
            -Q, --quote <command>  send command(s) to server before transfer
            --random-file <file>  file for reading random data from
            -r, --range <range>  retrieve only the bytes within RANGE
            --rate <max request rate>  request rate for serial transfers
            --raw  do HTTP "raw"; no transfer decoding
            -e, --referer <URL>  referrer URL
            -J, --remote-header-name  use the header-provided filename
            -O, --remote-name  write output to a file named as the remote file
            --remote-name-all  use the remote file name for all URLs
            -R, --remote-time  set the remote file's time on the local output
            --remove-on-error  remove output file on errors
            -X, --request <method>  specify request method to use
            --request-target <path>  specify the target for this request
            --resolve <[+]host:port:addr[,addr]>  resolve the host+port to this address
            --retry <num>  retry request if transient problems occur
            --retry-all-errors  retry all errors (use with --retry)
            --retry-connrefused  retry on connection refused (use with --retry)
            --retry-delay <seconds>  wait time between retries
            --retry-max-time <seconds>  retry only within this period
            --sasl-authzid <identity>  identity for SASL PLAIN authentication
            --sasl-ir  enable initial response in SASL authentication
            --service-name <name>  SPNEGO service name
            -S, --show-error  show error even when -s is used
            -s, --silent  silent mode
            --socks4 <host[:port]>  SOCKS4 proxy on given host + port
            --socks4a <host[:port]>  SOCKS4a proxy on given host + port
            --socks5 <host[:port]>  SOCKS5 proxy on given host + port
            --socks5-basic  enable username/password auth for SOCKS5 proxies
            --socks5-gssapi  enable GSS-API auth for SOCKS5 proxies
            --socks5-gssapi-nec  compatibility with NEC SOCKS5 server
            --socks5-gssapi-service <name>  SOCKS5 proxy service name for GSS-API
            --socks5-hostname <host[:port]>  SOCKS5 proxy, pass host name to proxy
            -Y, --speed-limit <speed>  stop transfers slower than this
            -y, --speed-time <seconds>  trigger 'speed-limit' abort after this time
            --ssl  try SSL/TLS
            --ssl-allow-beast  allow security flaw to improve interop
            --ssl-auto-client-cert  use auto client certificate (Schannel)
            --ssl-no-revoke  disable cert revocation checks (Schannel)
            --ssl-reqd  require SSL/TLS
            --ssl-revoke-best-effort  ignore missing/offline cert CRL dist points (Schannel)
            -2, --sslv2  use SSLv2
            -3, --sslv3  use SSLv3
            --stderr <file>  where to redirect stderr
            --styled-output  enable styled output for HTTP headers
            --suppress-connect-headers  suppress proxy CONNECT response headers
            --tcp-fastopen  use TCP Fast Open
            --tcp-nodelay  use the TCP_NODELAY option
            -t, --telnet-option <opt=val>  set telnet option
            --tftp-blksize <value>  set TFTP BLKSIZE option
            --tftp-no-options  do not send any TFTP options
            -z, --time-cond <time>  transfer based on a time condition
            --tls-max <VERSION>  set maximum allowed TLS version
            --tls13-ciphers <ciphersuite list>  TLS 1.3 cipher suites to use
            --tlsauthtype <type>  TLS authentication type
            --tlspassword <string>  TLS password
            --tlsuser <name>  TLS user name
            -1, --tlsv1  use TLSv1.0 or greater
            --tlsv1.0  use TLSv1.0 or greater
            --tlsv1.1  use TLSv1.1 or greater
            --tlsv1.2  use TLSv1.2 or greater
            --tlsv1.3  use TLSv1.3 or greater
            --tr-encoding  request compressed transfer encoding
            --trace <file>  write a debug trace to FILE
            --trace-ascii <file>  like --trace, but without hex output
            --trace-config <string>  details to log in trace/verbose output
            --trace-ids  add transfer and connection identifiers to trace/verbose output
            --trace-time  add time stamps to trace/verbose output
            --unix-socket <path>  connect through this Unix domain socket
            -T, --upload-file <file>  transfer local FILE to destination
            --url <url>  URL to work with
            --url-query <data>  add a URL query part
            -B, --use-ascii  use ASCII/text transfer
            -u, --user <user:password>  server user and password
            -A, --user-agent <name>  send User-Agent <name> to server
            --variable <[%]name=text/@file>  set variable
            -v, --verbose  make the operation more talkative
            -V, --version  show version number and quit
            -w, --write-out <format>  use output FORMAT after completion
            --xattr  store metadata in extended file attributes
        `,
        guess_values: true,
        values: #{
            "--cacert": "files", "--capath": "dirs", "-E": "files", "--key": "files", "--pubkey": "files",
            "--proxy-cert": "files", "--proxy-key": "files", "--proxy-cacert": "files", "--proxy-capath": "dirs",
            "--interface": "net:interfaces", "--dns-interface": "net:interfaces", "-H": "none",
            "--proxy-header": "none",
            "--cert-type": ["DER", "PEM", "ENG", "P12"], "--key-type": ["DER", "PEM", "ENG"],
            "--proxy-cert-type": ["DER", "PEM", "ENG", "P12"], "--proxy-key-type": ["DER", "PEM", "ENG"],
            "-X": ["GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH"],
            "--tls-max": ["default", "1.0", "1.1", "1.2", "1.3"], "--ftp-method": ["multicwd", "nocwd", "singlecwd"],
            "--delegation": ["none", "policy", "always"], "--krb": ["clear", "safe", "confidential", "private"],
            "-h": ["all", "auth", "connection", "curl", "dns", "file", "ftp", "http", "imap", "important", "ldap",
                "output", "pop3", "post", "proxy", "scp", "sftp", "smtp", "ssh", "telnet", "tftp", "tls", "upload",
                "verbose"],
        },
        args: ["none"],
    }
}

fn wget() {
    #{
        opts: `
            -V, --version  display the version of Wget and exit
            -h, --help  print this help
            -b, --background  go to background after startup
            -e, --execute=COMMAND  execute a .wgetrc-style command
            -o, --output-file=FILE  log messages to FILE
            -a, --append-output=FILE  append messages to FILE
            -d, --debug  print lots of debugging information
            -q, --quiet  quiet (no output)
            -v, --verbose  be verbose (this is the default)
            -nv, --no-verbose  turn off verboseness, without being quiet
            --report-speed=TYPE  output bandwidth as TYPE (bits)
            -i, --input-file=FILE  download URLs found in local or external FILE
            -F, --force-html  treat input file as HTML
            -B, --base=URL  resolves HTML input-file links (-i -F)
            --config=FILE  specify config file to use
            --no-config  do not read any config file
            --rejected-log=FILE  log reasons for URL rejection to FILE
            -t, --tries=NUMBER  set number of retries to NUMBER (0 unlimits)
            --retry-connrefused  retry even if connection is refused
            --retry-on-host-error  consider host errors as non-fatal, transient errors
            --retry-on-http-error=ERRORS  comma-separated list of HTTP errors to retry
            -O, --output-document=FILE  write documents to FILE
            -nc, --no-clobber  skip downloads that would overwrite existing files
            --no-netrc  don't try to obtain credentials from .netrc
            -c, --continue  resume getting a partially-downloaded file
            --start-pos=OFFSET  start downloading from zero-based position OFFSET
            --progress=TYPE  select progress gauge type
            --show-progress  display the progress bar in any verbosity mode
            -N, --timestamping  don't re-retrieve files unless newer than the local ones
            --no-if-modified-since  don't use conditional if-modified-since get
            --no-use-server-timestamps  don't set the local file's timestamp from the server's
            -S, --server-response  print server response
            --spider  don't download anything
            -T, --timeout=SECONDS  set all timeout values to SECONDS
            --dns-timeout=SECS  set the DNS lookup timeout to SECS
            --connect-timeout=SECS  set the connect timeout to SECS
            --read-timeout=SECS  set the read timeout to SECS
            -w, --wait=SECONDS  wait SECONDS between retrievals
            --waitretry=SECONDS  wait 1..SECONDS between retries of a retrieval
            --random-wait  wait from 0.5*WAIT...1.5*WAIT secs between retrievals
            --no-proxy  explicitly turn off proxy
            -Q, --quota=NUMBER  set retrieval quota to NUMBER
            --bind-address=ADDRESS  bind to ADDRESS (hostname or IP) on local host
            --limit-rate=RATE  limit download rate to RATE
            --no-dns-cache  disable caching DNS lookups
            --restrict-file-names=OS  restrict chars in file names to ones OS allows
            --ignore-case  ignore case when matching files/directories
            -4, --inet4-only  connect only to IPv4 addresses
            -6, --inet6-only  connect only to IPv6 addresses
            --prefer-family=FAMILY  connect first to addresses of the specified family
            --user=USER  set both ftp and http user to USER
            --password=PASS  set both ftp and http password to PASS
            --ask-password  prompt for passwords
            --use-askpass=COMMAND  the credential handler for requesting a username and password
            --no-iri  turn off IRI support
            --local-encoding=ENC  use ENC as the local encoding for IRIs
            --remote-encoding=ENC  use ENC as the default remote encoding
            --unlink  remove file before clobber
            --xattr  turn on storage of metadata in extended file attributes
            -nd, --no-directories  don't create directories
            -x, --force-directories  force creation of directories
            -nH, --no-host-directories  don't create host directories
            --protocol-directories  use protocol name in directories
            -P, --directory-prefix=PREFIX  save files to PREFIX/
            --cut-dirs=NUMBER  ignore NUMBER remote directory components
            --http-user=USER  set http user to USER
            --http-password=PASS  set http password to PASS
            --no-cache  disallow server-cached data
            --default-page=NAME  change the default page name (normally index.html)
            -E, --adjust-extension  save HTML/CSS documents with proper extensions
            --ignore-length  ignore 'Content-Length' header field
            --header=STRING  insert STRING among the headers
            --compression=TYPE  choose compression, one of auto, gzip and none
            --max-redirect=NUM  maximum redirections allowed per page
            --proxy-user=USER  set USER as proxy username
            --proxy-password=PASS  set PASS as proxy password
            --referer=URL  include 'Referer: URL' header in HTTP request
            --save-headers  save the HTTP headers to file
            -U, --user-agent=AGENT  identify as AGENT instead of Wget/VERSION
            --no-http-keep-alive  disable HTTP keep-alive (persistent connections)
            --no-cookies  don't use cookies
            --load-cookies=FILE  load cookies from FILE before session
            --save-cookies=FILE  save cookies to FILE after session
            --keep-session-cookies  load and save session (non-permanent) cookies
            --post-data=STRING  use the POST method; send STRING as the data
            --post-file=FILE  use the POST method; send contents of FILE
            --method=METHOD  use the HTTP method METHOD
            --body-data=STRING  send STRING as data
            --body-file=FILE  send contents of FILE. --method MUST be set
            --content-disposition  honor the Content-Disposition header when choosing file names
            --content-on-error  output the received content on server errors
            --auth-no-challenge  send Basic HTTP authentication information without waiting for a challenge
            --secure-protocol=PR  choose the secure protocol
            --https-only  only follow secure HTTPS links
            --no-check-certificate  don't validate the server's certificate
            --certificate=FILE  client certificate file
            --certificate-type=TYPE  client certificate type, PEM or DER
            --private-key=FILE  private key file
            --private-key-type=TYPE  private key type, PEM or DER
            --ca-certificate=FILE  file with the bundle of CAs
            --ca-directory=DIR  directory where hash list of CAs is stored
            --crl-file=FILE  file with bundle of CRLs
            --pinnedpubkey=FILE/HASHES  public key file, or hashes, to verify the peer against
            --random-file=FILE  file with random data for seeding the SSL PRNG
            --ciphers=STR  set the priority string (GnuTLS) or cipher list string (OpenSSL) directly
            --no-hsts  disable HSTS
            --hsts-file=FILE  path of the HSTS database
            --ftp-user=USER  set ftp user to USER
            --ftp-password=PASS  set ftp password to PASS
            --no-remove-listing  don't remove '.listing' files
            --no-glob  turn off FTP file name globbing
            --no-passive-ftp  disable the "passive" transfer mode
            --preserve-permissions  preserve remote file permissions
            --retr-symlinks  when recursing, get linked-to files (not dir)
            --ftps-implicit  use implicit FTPS (default port is 990)
            --ftps-resume-ssl  resume the SSL/TLS session of the control connection
            --ftps-clear-data-connection  cipher the control channel only; all the data will be in plaintext
            --ftps-fallback-to-ftp  fall back to FTP if FTPS is not supported in the target server
            --warc-file=FILENAME  save request/response data to a .warc.gz file
            --warc-header=STRING  insert STRING into the warcinfo record
            --warc-max-size=NUMBER  set maximum size of WARC files to NUMBER
            --warc-cdx  write CDX index files
            --warc-dedup=FILENAME  do not store records listed in this CDX file
            --no-warc-compression  do not compress WARC files with GZIP
            --no-warc-digests  do not calculate SHA1 digests
            --no-warc-keep-log  do not store the log file in a WARC record
            --warc-tempdir=DIRECTORY  the location of temporary files of the WARC writer
            -r, --recursive  specify recursive download
            -l, --level=NUMBER  maximum recursion depth (inf or 0 for infinite)
            --delete-after  delete files locally after downloading them
            -k, --convert-links  make links in downloaded HTML or CSS point to local files
            --convert-file-only  convert the file part of the URLs only (usually known as the basename)
            --backups=N  before writing file X, rotate up to N backup files
            -K, --backup-converted  before converting file X, back up as X.orig
            -m, --mirror  shortcut for -N -r -l inf --no-remove-listing
            -p, --page-requisites  get all images, etc
            --strict-comments  turn on strict (SGML) handling of HTML comments
            -A, --accept=LIST  comma-separated list of accepted extensions
            -R, --reject=LIST  comma-separated list of rejected extensions
            --accept-regex=REGEX  regex matching accepted URLs
            --reject-regex=REGEX  regex matching rejected URLs
            --regex-type=TYPE  regex type (posix|pcre)
            -D, --domains=LIST  comma-separated list of accepted domains
            --exclude-domains=LIST  comma-separated list of rejected domains
            --follow-ftp  follow FTP links from HTML documents
            --follow-tags=LIST  comma-separated list of followed HTML tags
            --ignore-tags=LIST  comma-separated list of ignored HTML tags
            -H, --span-hosts  go to foreign hosts when recursive
            -L, --relative  follow relative links only
            -I, --include-directories=LIST  list of allowed directories
            --trust-server-names  use the name specified by the redirection
            -X, --exclude-directories=LIST  list of excluded directories
            -np, --no-parent  don't ascend to the parent directory
        `,
        guess_values: true,
        values: #{
            "-P": "dirs", "--progress": ["bar", "bar:force", "bar:noscroll", "dot", "dot:default", "dot:binary",
                "dot:mega", "dot:giga"],
            "--report-speed": ["bits"], "--prefer-family": ["IPv6", "IPv4", "none"],
            "--restrict-file-names": ["unix", "windows", "nocontrol", "ascii", "lowercase", "uppercase"],
            "--compression": ["auto", "gzip", "none"],
            "--secure-protocol": ["auto", "SSLv2", "SSLv3", "TLSv1", "TLSv1_1", "TLSv1_2", "TLSv1_3", "PFS"],
            "--certificate-type": ["PEM", "DER"], "--private-key-type": ["PEM", "DER"],
            "--regex-type": ["posix", "pcre"],
            "--method": ["GET", "HEAD", "POST", "PUT", "DELETE", "OPTIONS", "PATCH"],
            "--use-askpass": "commands",
        },
        args: ["none"],
    }
}

fn ip() {
    let verbs = `
        show, list, lst, sh  show the objects
        add  add an object
        delete, del  delete an object
        change  change an object
        replace  add or change an object
        flush  flush the objects
        set  change the attributes of an object
        get  get a single object
        monitor  watch for changes
        help  show the help of the object
    `;
    let object = #{commands: verbs, args: ["net:ip_args"], subs: #{
        show: #{args: ["net:ip_args"]}, add: #{args: ["net:ip_args"]}, delete: #{args: ["net:ip_args"]},
        change: #{args: ["net:ip_args"]}, replace: #{args: ["net:ip_args"]}, flush: #{args: ["net:ip_args"]},
        set: #{args: ["net:ip_args"]}, get: #{args: ["net:ip_args"]}, monitor: #{args: ["none"]},
        help: #{args: ["none"]},
    }};
    let netns = #{
        commands: `
            list  list the namespaces
            add  add a namespace
            delete, del  delete a namespace
            exec  run a command in a namespace
            identify  show the namespace of a process
            pids  list the processes in a namespace
            set  assign an ID to a namespace
            attach  create a namespace from a process
            monitor  watch for changes
        `,
        subs: #{
            delete: #{args: ["net:netns"]}, exec: #{args: ["net:netns", "commands", "files"]},
            identify: #{args: ["pids"]}, pids: #{args: ["net:netns"]}, set: #{args: ["net:netns", "none"]},
            attach: #{args: ["none", "pids"]}, list: #{args: ["none"]}, add: #{args: ["none"]},
            monitor: #{args: ["none"]},
        },
    };
    #{
        single_dash: true,
        opts: `
            -V, -Version  print the version
            -h, -human, -human-readable  print human readable rates and sizes
            -b, -batch FILENAME  read the commands from FILENAME
            -force  don't stop at errors in batch mode
            -s, -stats, -statistics  output more information
            -d, -details  output more detailed information
            -l, -loops COUNT  the maximum number of loops of ip address flush
            -f, -family FAMILY  the protocol family to use
            -4  shortcut for -family inet
            -6  shortcut for -family inet6
            -B  shortcut for -family bridge
            -M  shortcut for -family mpls
            -0  shortcut for -family link
            -o, -oneline  output each record on a single line
            -r, -resolve  use the system's name resolver to print DNS names
            -n, -netns NETNS  switch to the network namespace NETNS
            -N, -Numeric  print numbers instead of names
            -a, -all  execute the command over all objects
            -c, -color[=WHEN]  use color output
            -t, -timestamp  display the current time with monitor
            -ts, -tshort  like -timestamp, in a shorter format
            -rc, -rcvbuf SIZE  set the netlink socket receive buffer size
            -iec  print human readable rates in IEC units
            -br, -brief  print only basic information in a tabular format
            -j, -json  output results in JSON
            -p, -pretty  output indented JSON
        `,
        values: #{
            "-f": ["inet", "inet6", "bridge", "mpls", "link"], "-l": "none", "-n": "net:netns",
            "-c": ["always", "auto", "never"], "-rc": "none",
        },
        commands: `
            address, addr, a  protocol (IP or IPv6) addresses of a device
            addrlabel  label configuration for protocol address selection
            link, l  network devices
            maddress, maddr  multicast addresses
            monitor  watch for netlink messages
            mroute  multicast routing cache entries
            mrule  rules in the multicast routing policy database
            neighbour, neighbor, neigh, n  ARP or NDISC cache entries
            netconf  network configuration
            netns  network namespaces
            nexthop  next hops
            ntable  neighbour cache operation
            route, r  routing table entries
            rule, ru  rules in the routing policy database
            tcpmetrics  TCP metrics
            token  tokenized interface identifiers
            tunnel  tunnels over IP
            tuntap  TUN and TAP devices
            vrf  virtual routing and forwarding devices
            xfrm  IPsec policies
        `,
        subs: #{
            address: object, addrlabel: object, link: object, maddress: object, mroute: object, mrule: object,
            neighbour: object, netconf: object, nexthop: object, ntable: object, route: object, rule: object,
            tcpmetrics: object, token: object, tunnel: object, tuntap: object, vrf: object, xfrm: object,
            netns: netns, monitor: #{args: [["all", "address", "link", "mroute", "neigh", "netconf", "route",
                "rule", "nsid", "nexthop"]]},
        },
    }
}

fn ss() {
    #{
        opts: `
            -h, --help  show the help
            -V, --version  output version information
            -n, --numeric  don't resolve service names
            -r, --resolve  resolve host names
            -a, --all  display all sockets
            -l, --listening  display listening sockets
            -o, --options  show timer information
            -e, --extended  show detailed socket information
            -m, --memory  show socket memory usage
            -p, --processes  show the processes using each socket
            -T, --threads  show the threads using each socket
            -i, --info  show internal TCP information
            --tipcinfo  show internal tipc socket information
            -s, --summary  show a summary of socket usage
            --tos  show tos and priority information
            --cgroup  show cgroup information
            -b, --bpf  show bpf filter socket information
            -E, --events  display sockets as they are destroyed
            -Z, --context  display the SELinux security contexts of tasks
            -z, --contexts  display the SELinux security contexts of tasks and sockets
            -N, --net=NETNS  switch to the network namespace NETNS
            -4, --ipv4  display only IP version 4 sockets
            -6, --ipv6  display only IP version 6 sockets
            -0, --packet  display PACKET sockets
            -t, --tcp  display only TCP sockets
            -M, --mptcp  display only MPTCP sockets
            -S, --sctp  display only SCTP sockets
            -u, --udp  display only UDP sockets
            -d, --dccp  display only DCCP sockets
            -w, --raw  display only RAW sockets
            -x, --unix  display only Unix domain sockets
            --tipc  display only TIPC sockets
            --vsock  display only vsock sockets
            --xdp  display only XDP sockets
            -f, --family=FAMILY  display the sockets of type FAMILY
            -K, --kill  forcibly close sockets, and display what was closed
            -H, --no-header  suppress the header line
            -O, --oneline  print the data of each socket on a single line
            --inet-sockopt  show various inet socket options
            -A, --query, --socket=QUERY  the socket tables to show
            -D, --diag=FILE  dump raw information about TCP sockets to FILE
            -F, --filter=FILE  read the filter from FILE
        `,
        values: #{
            "-N": "net:netns", "-f": ["inet", "inet6", "link", "unix", "netlink", "vsock", "tipc", "xdp"],
            "-A": [
                "all", "inet", "tcp", "mptcp", "udp", "raw", "unix", "unix_dgram", "unix_stream", "unix_seqpacket",
                "packet", "packet_raw", "packet_dgram", "netlink", "dccp", "sctp", "vsock_stream", "vsock_dgram",
                "tipc", "xdp",
            ],
        },
        args: [[
            "state", "established", "syn-sent", "syn-recv", "fin-wait-1", "fin-wait-2", "time-wait", "closed",
            "close-wait", "last-ack", "listening", "closing", "connected", "synchronized", "bucket", "big", "dst",
            "src", "dport", "sport",
        ]],
    }
}

fn ping() {
    #{
        opts: `
            -a  use audible ping
            -A  use adaptive ping
            -B  sticky source address
            -c COUNT  stop after COUNT replies
            -C  call connect() on socket creation
            -D  print timestamps
            -d  use the SO_DEBUG socket option
            -e IDENTIFIER  the identifier of the ping session
            -f  flood ping
            -h  print help and exit
            -H  force reverse DNS name resolution
            -I INTERFACE  the interface name or address
            -i INTERVAL  seconds between sending each packet
            -L  suppress loopback of multicast packets
            -l PRELOAD  send PRELOAD packets while waiting for replies
            -m MARK  tag the packets going out
            -M PMTUD  path MTU discovery (do, dont, want, probe)
            -n  no reverse DNS name resolution
            -O  report outstanding replies
            -p PATTERN  the contents of the padding bytes
            -q  quiet output
            -Q TCLASS  use quality of service TCLASS bits
            -s SIZE  send SIZE data bytes
            -S SIZE  use SIZE as the SO_SNDBUF socket option value
            -t TTL  the time to live
            -U  print user-to-user latency
            -v  verbose output
            -V  print version and exit
            -w DEADLINE  exit after DEADLINE seconds
            -W TIMEOUT  the time to wait for a response
            -4  use IPv4
            -b  allow pinging broadcast
            -R  record route
            -T TIMESTAMP  the timestamp option (tsonly, tsandaddr, tsprespec)
            -6  use IPv6
            -F FLOWLABEL  the flow label
            -N NODEINFO  use IPv6 node info query
        `,
        values: #{
            "-c": "none", "-e": "none", "-I": "net:interfaces", "-i": "none", "-l": "none", "-m": "none",
            "-M": ["do", "dont", "want", "probe"], "-p": "none", "-Q": "none", "-s": "none", "-S": "none",
            "-t": "none", "-w": "none", "-W": "none", "-T": ["tsonly", "tsandaddr", "tsprespec"], "-F": "none",
            "-N": "none",
        },
        args: ["hosts"],
    }
}

fn dig() {
    #{
        opts: `
            -4  use IPv4 query transport only
            -6  use IPv6 query transport only
            -b ADDRESS  bind to the source address
            -c CLASS  the query class
            -f FILENAME  batch mode: read the queries from FILENAME
            -k KEYFILE  the TSIG key file
            -m  enable memory usage debugging
            -p PORT  the port number
            -q NAME  the query name
            -r  do not read ~/.digrc
            -t TYPE  the query type
            -u  display times in microseconds
            -x ADDR  reverse lookup of ADDR
            -y KEY  the TSIG key, as [hmac:]name:key
            -h  print the help and exit
            -v  print the version and exit
        `,
        values: #{
            "-b": "none", "-c": ["IN", "CH", "HS"], "-p": "none", "-q": "hosts", "-t": "net:record_types",
            "-x": "none", "-y": "none",
        },
        args: ["net:dig_args"],
    }
}

fn ssh_add() {
    #{
        opts: `
            -c  confirm each use of the identity
            -D  delete all identities from the agent
            -d  delete the identities from the agent
            -E HASH  the hash algorithm of fingerprints (md5, sha256)
            -e PKCS11  remove the keys of the PKCS#11 provider
            -H HOSTKEY_FILE  the known hosts file for destination constraints
            -h CONSTRAINT  a destination constraint
            -K  load resident keys from a FIDO authenticator
            -k  load only the keys, not certificates
            -L  list the public keys of all identities in the agent
            -l  list the fingerprints of all identities in the agent
            -q  be quiet
            -S PROVIDER  the FIDO authenticator middleware library
            -s PKCS11  add the keys of the PKCS#11 provider
            -T  test that the private keys of the public keys are usable
            -t LIFE  the lifetime of the identities
            -v  verbose mode
            -X  unlock the agent
            -x  lock the agent with a password
        `,
        values: #{"-E": ["md5", "sha256"], "-h": "none", "-t": "none"},
    }
}

fn ssh_keygen() {
    #{
        opts: `
            -A  generate host keys of all default types
            -a ROUNDS  the number of KDF rounds
            -B  show the bubblebabble digest of a key
            -b BITS  the number of bits of the key
            -C COMMENT  a new comment
            -c  change the comment of a key
            -D PKCS11  download the public keys of the PKCS#11 library
            -E HASH  the hash algorithm of fingerprints (md5, sha256)
            -e  export a key in another format
            -F HOSTNAME  search for HOSTNAME in a known_hosts file
            -f FILENAME  the file of the key
            -g  use generic DNS format with -r
            -H  hash a known_hosts file
            -h  make a host certificate when signing a key
            -I CERTIFICATE_IDENTITY  the key identity when signing a key
            -i  import a key from another format
            -K  download resident keys from a FIDO authenticator
            -k  generate a KRL file
            -L  print the contents of certificates
            -l  show the fingerprint of a key
            -M COMMAND  generate or screen DH-GEX moduli (generate, screen)
            -m FORMAT  the key format (RFC4716, PKCS8, PEM)
            -N NEW_PASSPHRASE  the new passphrase
            -n PRINCIPALS  the principals of a certificate
            -O OPTION  a key or certificate option
            -P PASSPHRASE  the old passphrase
            -p  change the passphrase of a key
            -Q  test whether keys have been revoked in a KRL
            -q  be quiet
            -R HOSTNAME  remove the keys of HOSTNAME from a known_hosts file
            -r HOSTNAME  print the SSHFP record of HOSTNAME
            -s CA_KEY  sign a public key with a CA key
            -t TYPE  the type of key to create
            -U  the CA key is in ssh-agent
            -u  update a KRL
            -V VALIDITY_INTERVAL  the validity interval of a certificate
            -v  verbose mode
            -w PROVIDER  the FIDO authenticator middleware library
            -Y COMMAND  sign or verify data (sign, verify, find-principals, check-novalidate, match-principals)
            -y  print the public key of a private key
            -Z CIPHER  the cipher to encrypt the private key with
            -z SERIAL_NUMBER  the serial number of a certificate
        `,
        values: #{
            "-a": "none", "-b": "none", "-C": "none", "-D": "files", "-E": ["md5", "sha256"], "-F": "hosts",
            "-I": "none", "-M": ["generate", "screen"], "-m": ["RFC4716", "PKCS8", "PEM"], "-N": "none",
            "-n": "none", "-O": "none", "-P": "none", "-R": "hosts", "-r": "hosts",
            "-t": ["ed25519", "ed25519-sk", "ecdsa", "ecdsa-sk", "rsa", "dsa"], "-V": "none", "-w": "files",
            "-Y": ["sign", "verify", "find-principals", "check-novalidate", "match-principals"],
            "-Z": ["aes256-ctr", "aes128-ctr", "aes192-ctr", "aes256-gcm@openssh.com", "chacha20-poly1305@openssh.com"],
            "-z": "none",
        },
    }
}

fn ssh_copy_id() {
    #{
        opts: `
            -f  copy the keys without checking whether they are installed
            -n  dry run: copy no keys
            -s  use sftp instead of running commands on the host
            -x  enable -x in the script, for debugging
            -h, -?  print the help
            -i IDENTITY_FILE  the identity to copy
            -p PORT  the port to connect to
            -F CONFIG  an alternative ssh_config file
            -t TARGET_PATH  the path of the authorized keys file on the host
            -o SSH_OPTION  an option for ssh
        `,
        values: #{"-p": "none", "-t": "none", "-o": "none"},
        args: ["hosts"],
    }
}

fn gpg() {
    let keys = ["net:keys"];
    #{
        opts: `
            -s, --sign  make a signature
            --clear-sign  make a clear text signature
            -b, --detach-sign  make a detached signature
            -e, --encrypt  encrypt data
            -c, --symmetric  encrypt with a symmetric cipher only
            -d, --decrypt  decrypt data (the default)
            --verify  verify a signature
            -k, --list-keys  list keys
            --list-signatures  list keys and signatures
            --check-signatures  list and check key signatures
            --fingerprint  list keys and fingerprints
            -K, --list-secret-keys  list secret keys
            --generate-key  generate a new key pair
            --quick-generate-key  quickly generate a new key pair
            --quick-add-uid  quickly add a new user ID
            --quick-revoke-uid  quickly revoke a user ID
            --quick-set-expire  quickly set a new expiration date
            --full-generate-key  full featured key pair generation
            --generate-revocation  generate a revocation certificate
            --delete-keys  remove keys from the public keyring
            --delete-secret-keys  remove keys from the secret keyring
            --quick-sign-key  quickly sign a key
            --quick-lsign-key  quickly sign a key locally
            --quick-revoke-sig  quickly revoke a key signature
            --sign-key  sign a key
            --lsign-key  sign a key locally
            --edit-key  sign or edit a key
            --change-passphrase  change a passphrase
            --export  export keys
            --export-secret-keys  export secret keys
            --send-keys  export keys to a keyserver
            --receive-keys, --recv-keys  import keys from a keyserver
            --search-keys  search for keys on a keyserver
            --refresh-keys  update all keys from a keyserver
            --import  import or merge keys
            --card-status  print the card status
            --edit-card  change data on a card
            --change-pin  change a card's PIN
            --update-trustdb  update the trust database
            --print-md ALGO  print message digests
            --server  run in server mode
            --tofu-policy VALUE  set the TOFU policy of a key
            -v, --verbose  verbose
            -q, --quiet  be somewhat more quiet
            --options FILE  read options from FILE
            --log-file FILE  write server mode logs to FILE
            --default-key NAME  use NAME as the default secret key
            --encrypt-to NAME  also encrypt to the user ID NAME
            --group SPEC  set up email aliases
            --openpgp  use strict OpenPGP behavior
            -n, --dry-run  do not make any changes
            -i, --interactive  prompt before overwriting
            -a, --armor  create ASCII armored output
            -o, --output FILE  write the output to FILE
            --textmode  use canonical text mode
            -z N  set the compress level to N (0 disables)
            --auto-key-locate MECHANISMS  use MECHANISMS to locate keys by mail address
            --auto-key-import  import missing keys from a signature
            --include-key-block  include the public key in signatures
            --disable-dirmngr  disable all access to the dirmngr
            -r, --recipient USER-ID  encrypt for USER-ID
            -u, --local-user USER-ID  use USER-ID to sign or decrypt
            --batch  never ask questions
            --yes  assume yes to most questions
            --keyserver NAME  use the keyserver NAME
            --homedir DIR  use DIR as the home directory
            --pinentry-mode MODE  how to ask for passphrases
            --with-colons  list in a machine-readable format
            --keyid-format FORMAT  the format of key IDs
            --version  print the version
            -h, --help  print the help
        `,
        values: #{
            "--print-md": ["MD5", "SHA1", "RIPEMD160", "SHA256", "SHA384", "SHA512", "SHA224"],
            "--tofu-policy": ["auto", "good", "unknown", "bad", "ask"], "--default-key": "net:secret_keys",
            "--encrypt-to": "net:keys", "--group": "none", "-z": "none", "--auto-key-locate": "none",
            "-r": "net:keys", "-u": "net:secret_keys", "--keyserver": "none", "--homedir": "dirs",
            "--pinentry-mode": ["default", "ask", "cancel", "error", "loopback"],
            "--keyid-format": ["none", "short", "0xshort", "long", "0xlong"],
        },
        args_if: #{
            "--list-keys": keys, "--list-signatures": keys, "--check-signatures": keys, "--fingerprint": keys,
            "--list-secret-keys": ["net:secret_keys"], "--quick-add-uid": keys, "--quick-revoke-uid": keys,
            "--quick-set-expire": keys, "--generate-revocation": keys, "--delete-keys": keys,
            "--delete-secret-keys": ["net:secret_keys"], "--quick-sign-key": keys, "--quick-lsign-key": keys,
            "--sign-key": keys, "--lsign-key": keys, "--edit-key": keys, "--change-passphrase": ["net:secret_keys"],
            "--export": keys, "--export-secret-keys": ["net:secret_keys"], "--send-keys": keys,
            "--receive-keys": ["none"], "--search-keys": ["none"], "--refresh-keys": keys,
        },
    }
}

fn openssl() {
    #{
        opts: ``,
        commands: `
            asn1parse  parse an ASN.1 sequence
            ca  a minimal certificate authority
            ciphers  list the cipher suites
            cms  CMS (S/MIME) utility
            crl  CRL utility
            crl2pkcs7  convert a CRL to PKCS#7
            dgst  message digests and signatures
            dhparam  DH parameters
            ec  EC key processing
            ecparam  EC parameters
            enc  symmetric encryption and decryption
            errstr  look up error codes
            gendsa  generate DSA keys
            genpkey  generate private keys
            genrsa  generate RSA keys
            help  list the commands
            info  print information about the build
            kdf  key derivation functions
            list  list algorithms and features
            mac  message authentication codes
            ocsp  OCSP utility
            passwd  hash passwords
            pkcs12  PKCS#12 utility
            pkcs7  PKCS#7 utility
            pkcs8  PKCS#8 utility
            pkey  public and private key processing
            pkeyparam  public key parameters
            pkeyutl  public key operations
            prime  test primes
            rand  generate random bytes
            rehash  create symbolic links to certificates by their hashes
            req  certificate requests
            rsa  RSA key processing
            s_client  SSL/TLS client
            s_server  SSL/TLS server
            s_time  SSL/TLS timing
            sess_id  SSL session data
            smime  S/MIME utility
            speed  benchmark algorithms
            spkac  SPKAC utility
            srp  SRP password file
            storeutl  load certificates and keys from URIs
            ts  time stamp authority
            verify  verify certificates
            version  print the version
            x509  certificate utility
        `,
        sub_spec: "net:openssl",
    }
}

fn spec(cmd) {
    switch cmd {
        "curl" => curl(),
        "wget" => wget(),
        "ip" => ip(),
        "ss" => ss(),
        "ping" | "ping6" => ping(),
        "dig" => dig(),
        "ssh-add" => ssh_add(),
        "ssh-keygen" => ssh_keygen(),
        "ssh-copy-id" => ssh_copy_id(),
        "gpg" | "gpg2" => gpg(),
        "openssl" => openssl(),
        _ => throw `no spec for ${cmd}`,
    }
}
