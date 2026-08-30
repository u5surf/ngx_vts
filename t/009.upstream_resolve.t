# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Ported from nginx-module-vts t/028.upstream_resolve.t, including its test
# resolver (t/dns_server.pl).
#
# The `resolve` parameter of the upstream server directive needs nginx 1.27.3
# or later, so this file skips itself on anything older.
#
# TEST 2 answers the queries of nginx itself and gives the name a second
# address half way through, which is what a re-resolve does to an upstream
# group. A peer that is replaced has to keep the numbers it collected: those
# requests happened, and an operator looking at a spike wants to see them even
# after the address is gone from the group.
#
# This module keys its upstream series on the peer address, so a replaced peer
# keeps its own series rather than being folded into the new one.

use Test::Nginx::Socket;
use Socket;
use POSIX ();

my $nginx = $ENV{TEST_NGINX_BINARY} || 'nginx';
my $version = `$nginx -v 2>&1` || '';
my $supported = 0;

if ($version =~ m{^nginx version: nginx/(\d+)\.(\d+)\.(\d+)}) {
    $supported = ($1 * 1000000 + $2 * 1000 + $3) >= 1027003;
}

plan skip_all => "upstream resolve is not supported by: $version"
    unless $supported;

# the test resolver

my $DNSPort    = 18653;
my $FirstAddr  = '127.0.0.1';   # port 1984 of the test server answers here
my $SecondAddr = '127.0.0.2';
my $SwitchIn   = 4;             # seconds until the name gets the second address

# Sends a query and tells whether the test resolver answers it.
sub dns_ready {
    my $sock;

    socket($sock, PF_INET, SOCK_DGRAM, getprotobyname('udp')) or return 0;

    my $to = sockaddr_in($DNSPort, inet_aton('127.0.0.1'));
    my $query = pack('n n n n n n', 0x2a2a, 0x0100, 1, 0, 0, 0)
                . join('', map { chr(length $_) . $_ } qw(vts-test example))
                . "\0" . pack('n n', 1, 1);

    for (1 .. 30) {
        next unless send($sock, $query, 0, $to);

        my $rin = '';
        vec($rin, fileno($sock), 1) = 1;

        if (select($rin, undef, undef, 0.1)) {
            my $answer;
            recv($sock, $answer, 512, 0);
            close $sock;
            return 1;
        }
    }

    close $sock;

    return 0;
}

# The resolver runs as its own process: a plain fork of the test is not safe
# on every platform.
my $dns_pid = fork();

defined $dns_pid or plan skip_all => "fork() failed: $!";

if ($dns_pid == 0) {
    my $server = __FILE__;
    $server =~ s{[^/]+$}{dns_server.pl};

    # the _exit() is the path where exec() itself failed; writing it as the
    # alternative rather than the next statement is also what keeps perl from
    # warning that it is unlikely to be reached
    exec($^X, $server, $DNSPort, $SwitchIn, $FirstAddr, $SecondAddr)
        or POSIX::_exit(1);
}

END {
    if ($dns_pid) {
        # waitpid() must not carry the status of the resolver into the exit
        # status of the test
        local $?;

        kill 'TERM', $dns_pid;
        waitpid($dns_pid, 0);
    }
}

plan skip_all => "the test resolver does not answer on 127.0.0.1:$DNSPort"
    unless dns_ready();

repeat_each(2);
plan tests => 28;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: a peer resolved at run time is counted
--- http_config
    vts_zone main 1m;

    resolver 127.0.0.1:18653 valid=1s ipv6=off;
    resolver_timeout 2s;

    upstream backend {
        zone backend 1m;
        server vts-test.example:1984 resolve;
    }
--- config
    location /ok     { return 200 "OK"; }
    location /up     { proxy_pass http://backend/ok; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\AOK\z/,
    qr/nginx_vts_upstream_requests_total\{upstream="backend",server="127\.0\.0\.1:1984"\} [1-9]\d*/,
]



=== TEST 2: a peer replaced by a re-resolve keeps its statistics
# Only 127.0.0.1 has a listener, so the attempts that the balancer sends to the
# second address fail. That is beside the point here and the statuses are left
# unasserted: what matters is that both peers hold their own numbers. A short
# connect timeout keeps those attempts from stalling the client socket.
--- http_config
    vts_zone main 1m;

    resolver 127.0.0.1:18653 valid=1s ipv6=off;
    resolver_timeout 2s;

    upstream backend {
        zone backend 1m;
        server vts-test.example:1984 resolve;
    }
--- config
    location /ok     { return 200 "OK"; }
    location /up     {
        proxy_connect_timeout 1s;
        proxy_next_upstream off;
        proxy_pass http://backend/ok;
    }
    location /status { vts_status; }
--- wait: 6
--- request eval
['GET /up', 'GET /up', 'GET /up', 'GET /up', 'GET /status']
--- error_code_like eval
[qr/^(200|502|504)$/, qr/^(200|502|504)$/, qr/^(200|502|504)$/, qr/^(200|502|504)$/, qr/^200$/]
--- response_body_like eval
[
    qr//,
    qr//,
    qr//,
    qr//,
    qr/(?=.*server="127\.0\.0\.2:1984")(?=.*server="127\.0\.0\.1:1984"\} [1-9]\d*)/s,
]
