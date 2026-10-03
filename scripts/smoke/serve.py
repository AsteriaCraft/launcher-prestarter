"""Serves a directory on 127.0.0.1 for the smoke tests: serve.py <port> <directory>.

Unlike `python -m http.server`, it never looks up the host name (`socket.getfqdn` in `HTTPServer.server_bind`),
which on macOS asks for local-network access and covers the screen with a dialog.
"""

import functools
import http.server
import socketserver
import sys


class LoopbackServer(http.server.ThreadingHTTPServer):
    def server_bind(self) -> None:
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = self.server_address[:2]


def main() -> None:
    port, directory = int(sys.argv[1]), sys.argv[2]
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=directory)
    with LoopbackServer(("127.0.0.1", port), handler) as server:
        server.serve_forever()


if __name__ == "__main__":
    main()
