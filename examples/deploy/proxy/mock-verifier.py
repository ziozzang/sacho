#!/usr/bin/env python3
"""TEST ONLY: stand-in for an authentication service, with no real sign-in.

It lets you prove the proxy wiring before connecting a real identity provider.
Anyone who can set the cookie "session=admin" passes, so never expose it.

    python3 mock-verifier.py 127.0.0.1:4180 admin@example.com reader@example.com

  session=admin   GET /oauth2/auth -> 202 with X-Auth-Request-Email: <first address>
  session=reader  GET /oauth2/auth -> 202 with X-Auth-Request-Email: <second address>
  otherwise       GET /oauth2/auth -> 401
  GET /optional-identity           -> 200, with the email only for a valid session
  GET /oauth2/sign_in              -> a placeholder sign-in page
"""
import http.cookies
import http.server
import sys

host, port = sys.argv[1].rsplit(":", 1)
USERS = {"admin": sys.argv[2], "reader": sys.argv[3]}


class Verifier(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def email(self):
        cookies = http.cookies.SimpleCookie(self.headers.get("Cookie", ""))
        session = cookies.get("session")
        return USERS.get(session.value) if session else None

    def answer(self, code, email=None, body=b""):
        self.send_response(code)
        if email:
            self.send_header("X-Auth-Request-Email", email)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        email = self.email()
        if path == "/oauth2/auth":
            self.answer(202 if email else 401, email)
        elif path == "/optional-identity":
            self.answer(200, email)
        elif path == "/oauth2/sign_in":
            self.answer(200, body=b"placeholder sign-in page\n")
        else:
            self.answer(404)


http.server.ThreadingHTTPServer((host, int(port)), Verifier).serve_forever()
