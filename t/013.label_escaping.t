# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Prometheus label escaping.
#
# Five labels carry names from the configuration rather than from this module:
# hostname, the server zone, the cache zone, the upstream group and the peer
# address. nginx's configuration parser accepts a quoted token containing
# anything, so any of them can hold a `"` or a `\` - the quote just has to open
# the token, as it does below.
#
# The exposition format has no literal for either character inside a label
# value. An unescaped one does not corrupt only its own line: the value ends
# early, the rest of the line is garbage, and a scraper rejects the whole
# response. One awkward name would take every other metric down with it.
#
# The format defines exactly three escapes - \\, \" and \n. These tests cover
# the two a configuration can actually produce.
#
# The server blocks under test live in `http_config` rather than in `config`
# because the harness writes its own `server_name localhost` into the latter,
# and this module keys a zone on the first name a server block declares.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 16;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: a server_name holding a quote is escaped in the zone label
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        server_name "a\"b";
        location / { return 200 "ok"; }
    }
--- config
    location /hit    { proxy_pass http://127.0.0.1:1985/; }
    location /status { vts_status; }
--- request eval
['GET /hit', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    # The backslash is in the response; qr needs it doubled.
    qr{nginx_vts_server_requests_total\{zone="a\\"b"\} 1},
]
--- error_code eval
[200, 200]



=== TEST 2: a peer address holding a quote is escaped in the server label
--- http_config
    vts_zone main 1m;

    server {
        listen "unix:/tmp/vts-t013-a\"b.sock";
        location / { return 200 "ok"; }
    }

    upstream u {
        server "unix:/tmp/vts-t013-a\"b.sock";
    }
--- config
    location /up     { proxy_pass http://u/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    qr{nginx_vts_upstream_requests_total\{upstream="u",server="unix:/tmp/vts-t013-a\\"b\.sock"\} 1},
]
--- error_code eval
[200, 200]



=== TEST 3: server_up carries the same escaping as the counters
--- http_config
    vts_zone main 1m;

    server {
        listen "unix:/tmp/vts-t013-b\"c.sock";
        location / { return 200 "ok"; }
    }

    upstream u {
        server "unix:/tmp/vts-t013-b\"c.sock";
    }
--- config
    location /up     { proxy_pass http://u/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    # server_up comes from the group walk rather than from the counters, so it
    # is a separate formatting path and needs its own assertion.
    qr{nginx_vts_upstream_server_up\{upstream="u",server="unix:/tmp/vts-t013-b\\"c\.sock"\} 1},
]
--- error_code eval
[200, 200]



=== TEST 4: with both awkward names present, every line still parses
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        server_name "c\"d";
        location / { return 200 "ok"; }
    }

    server {
        listen "unix:/tmp/vts-t013-c\"d.sock";
        location / { return 200 "ok"; }
    }

    upstream u {
        server "unix:/tmp/vts-t013-c\"d.sock";
    }
--- config
    location /up     { proxy_pass http://u/; }
    location /hit    { proxy_pass http://127.0.0.1:1985/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    # Every label value opens and closes cleanly: between the opening quote
    # and the closing one there is no bare " and no bare \ outside one of the
    # three escapes. A single unescaped name anywhere fails this.
    qr/\A(?:\#[^\n]*\n|[a-z_]+(?:\{(?:[a-z_]+="(?:[^"\\\n]|\\[\\"n])*",?)+\})?\ [^\n]*\n|\n)+\z/,
]
--- error_code eval
[200, 200]
