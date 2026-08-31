# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# nginx_vts_upstream_server_up.
#
# Whether a peer is in rotation is a property of the upstream group, not of
# anything a request did, so it cannot come from r->upstream_states like the
# counters do. It is read off uscf->peer.data instead, the same two conditions
# the original module uses: the configured `down` parameter, or enough
# failures counted against max_fails for nginx to have taken the peer out.
#
# Before that walk existed the flag was never set and the gauge read 1 for
# every peer, including addresses nothing was listening on.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 24;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: a peer configured down reads as down
--- http_config
    vts_zone main 1m;

    server { listen 1985; location / { return 200 "ok"; } }

    upstream backend {
        server 127.0.0.1:1981 down;
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/nginx_vts_upstream_server_up\{upstream="backend",server="127\.0\.0\.1:1981"\} 0/,
]



=== TEST 2: a peer that answers reads as up
--- http_config
    vts_zone main 1m;

    server { listen 1985; location / { return 200 "ok"; } }

    upstream backend {
        server 127.0.0.1:1981 down;
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/nginx_vts_upstream_server_up\{upstream="backend",server="127\.0\.0\.1:1985"\} 1/,
]



=== TEST 3: enough failures take a peer out
--- http_config
    vts_zone main 1m;

    server { listen 1985; location / { return 200 "ok"; } }

    upstream backend {
        server 127.0.0.1:1981 max_fails=1;
        server 127.0.0.1:1985 backup;
    }
--- config
    location /up {
        proxy_next_upstream error timeout;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/\Aok\z/,
    qr/nginx_vts_upstream_server_up\{upstream="backend",server="127\.0\.0\.1:1981"\} 0/,
]



=== TEST 4: max_fails=0 leaves a failing peer in rotation
# Zero turns failure counting off, which nginx reads as never taking the peer
# out. The comparison against fails has to be guarded for it, or 0 >= 0 holds
# from the first request and every such peer reads as down.
--- http_config
    vts_zone main 1m;

    server { listen 1985; location / { return 200 "ok"; } }

    upstream backend {
        server 127.0.0.1:1981 max_fails=0;
        server 127.0.0.1:1985 backup;
    }
--- config
    location /up {
        proxy_next_upstream error timeout;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/\Aok\z/,
    qr/nginx_vts_upstream_server_up\{upstream="backend",server="127\.0\.0\.1:1981"\} 1/,
]



=== TEST 5: a peer that has served nothing still has a series
# The counters only know peers that appeared in r->upstream_states. Reading the
# group means an idle peer is reported as up rather than being absent.
--- http_config
    vts_zone main 1m;

    server { listen 1985; location / { return 200 "ok"; } }
    server { listen 1986; location / { return 200 "spare"; } }

    upstream backend {
        server 127.0.0.1:1985;
        server 127.0.0.1:1986 down;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/nginx_vts_upstream_server_up\{upstream="backend",server="127\.0\.0\.1:1986"\} 0/,
]
