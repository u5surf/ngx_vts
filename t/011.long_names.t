# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Key lengths, from nginx-module-vts t/037.upstream_long_peer_name.t and the
# concern behind t/026.long_names.t.
#
# The original sized the scratch buffer its upstream display builds keys in for
# a peer name no longer than an address and a port, which a unix socket path
# passes easily. It overflowed, and quietly: the key was then taken from the
# same pool with ngx_pcalloc, zeroing a region overlapping the tail just
# written, so the name came out cut short, the lookup missed, and the peer read
# as though it had served nothing.
#
# This module copies the peer out of r->upstream_states into a fixed buffer and
# skips anything that does not fit, so there is no overflow to have. What these
# blocks establish is the other half: that a name which does fit is kept whole,
# and what happens to one that does not.
#
# 026 itself does not port - every one of its blocks is about a filter name,
# and there are no filter directives here.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 16;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: a peer over a unix socket keeps its whole path
--- http_config
    vts_zone main 1m;

    upstream u {
        server unix:/tmp/vts-t011-a-deliberately-long-unix-domain-socket-path-for-the-key.sock;
    }

    server {
        listen unix:/tmp/vts-t011-a-deliberately-long-unix-domain-socket-path-for-the-key.sock;
        location / { return 200 "backend:OK"; }
    }
--- config
    location /up     { proxy_pass http://u/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Abackend:OK\z/,
    qr{nginx_vts_upstream_requests_total\{upstream="u",server="unix:/tmp/vts-t011-a-deliberately-long-unix-domain-socket-path-for-the-key\.sock"\} [1-9]\d*},
]



=== TEST 2: a server name that fits is the zone it is counted under
--- http_config
    vts_zone main 1m;
    server_names_hash_bucket_size 1024;

    server {
        listen $TEST_NGINX_SERVER_PORT;
        server_name uuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuu.example;
        location /hello  { return 200 "hello"; }
        location /status { vts_status; }
    }
--- config
    location /nothing { return 200 "x"; }
--- request eval
['GET /hello', 'GET /status']
--- more_headers eval
["Host: uuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuu.example\n", "Host: uuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuu.example\n"]
--- response_body_like eval
[
    qr/\Ahello\z/,
    qr/nginx_vts_server_requests_total\{zone="uuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuu.example"\} [1-9]\d*/,
]



=== TEST 3: a name too long for the key falls back rather than being dropped
# The buffer holds 255 bytes and this name is 308, so it cannot be the key. The
# request is still counted, against the default zone - losing the request
# outright would be the worse of the two, since a counter that silently stops
# is harder to notice than one that reads high.
--- http_config
    vts_zone main 1m;
    server_names_hash_bucket_size 1024;

    server {
        listen $TEST_NGINX_SERVER_PORT;
        server_name oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo.example;
        location /hello  { return 200 "hello"; }
        location /status { vts_status; }
    }
--- config
    location /nothing { return 200 "x"; }
--- request eval
['GET /hello', 'GET /status']
--- more_headers eval
["Host: oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo.example\n", "Host: oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo.example\n"]
--- response_body_like eval
[
    qr/\Ahello\z/,
    qr/nginx_vts_server_requests_total\{zone="_"\} [1-9]\d*/,
]



=== TEST 4: and the oversized name is nowhere in the output
--- http_config
    vts_zone main 1m;
    server_names_hash_bucket_size 1024;

    server {
        listen $TEST_NGINX_SERVER_PORT;
        server_name oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo.example;
        location /hello  { return 200 "hello"; }
        location /status { vts_status; }
    }
--- config
    location /nothing { return 200 "x"; }
--- request eval
['GET /hello', 'GET /status']
--- more_headers eval
["Host: oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo.example\n", "Host: oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo.example\n"]
--- response_body_unlike eval
[
    qr/this pattern is never in the body/,
    qr/zone="o{20}/,
]

