#!/usr/bin/env python3
"""
Simple HTTP server with COOP/COEP headers for SharedArrayBuffer support.
Required for running zcash-wallet-wasm in the browser.

Usage:
    python3 serve.py [port] [host]

Default: 0.0.0.0:8000 (allows remote access)
"""

import http.server
import socketserver
import mimetypes
import os
import gzip
import io
import argparse

class CORSRequestHandler(http.server.SimpleHTTPRequestHandler):
    # Add custom MIME types
    extensions_map = {
        '.wasm': 'application/wasm',
        '.js': 'application/javascript',
        '.mjs': 'application/javascript',
        '.json': 'application/json',
        '.html': 'text/html',
        '.css': 'text/css',
        '.png': 'image/png',
        '.jpg': 'image/jpeg',
        '.svg': 'image/svg+xml',
        '.ico': 'image/x-icon',
    }

    def guess_type(self, path):
        """Override to ensure correct MIME types"""
        base, ext = os.path.splitext(path)
        if ext in self.extensions_map:
            return self.extensions_map[ext]
        return super().guess_type(path)

    def end_headers(self):
        # Required headers for SharedArrayBuffer
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        # CORS headers for remote access
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', '*')
        self.send_header('Cross-Origin-Resource-Policy', 'cross-origin')
        # Cache control
        self.send_header('Cache-Control', 'no-cache, no-store, must-revalidate')
        super().end_headers()

    def do_OPTIONS(self):
        self.send_response(200)
        self.end_headers()

    def log_message(self, format, *args):
        """Override to show detailed logs for debugging"""
        # Show the full request path and response code
        message = format % args
        print(f"[{self.log_date_time_string()}] {message}")

    def do_GET(self):
        """Override to handle special cases, compression, and log requests"""
        # Log the request
        print(f"    → GET {self.path}")

        # Special case: when Worker requests /pkg/, serve the main module
        # This is needed because wasm-bindgen-rayon uses import('../../..')
        # which resolves to /pkg/ instead of /pkg/zcash_wallet_wasm.js
        if self.path.rstrip('/') in ('/pkg', '/pkg-sequential', '/pkg-proof', '/pkg-proof-sequential'):
            self.path = self.path.rstrip('/') + '/zcash_wallet_wasm.js'
            print(f"    ↪ Redirecting to {self.path}")

        # Check if client accepts gzip encoding
        accept_encoding = self.headers.get('Accept-Encoding', '')
        supports_gzip = 'gzip' in accept_encoding

        # For large files (.wasm, .js), use gzip compression if supported
        if supports_gzip and (self.path.endswith('.wasm') or self.path.endswith('.js')):
            # Try to serve compressed version
            path = self.translate_path(self.path)
            try:
                with open(path, 'rb') as f:
                    content = f.read()
                    # Compress content
                    compressed = io.BytesIO()
                    with gzip.GzipFile(fileobj=compressed, mode='wb', compresslevel=6) as gz:
                        gz.write(content)
                    compressed_data = compressed.getvalue()

                    # Send response with gzip encoding
                    self.send_response(200)
                    self.send_header('Content-Type', self.guess_type(path))
                    self.send_header('Content-Encoding', 'gzip')
                    self.send_header('Content-Length', len(compressed_data))
                    self.end_headers()
                    self.wfile.write(compressed_data)

                    orig_size = len(content) / (1024 * 1024)
                    comp_size = len(compressed_data) / (1024 * 1024)
                    ratio = (1 - len(compressed_data) / len(content)) * 100
                    print(f"    ✓ Compressed {orig_size:.1f}MB → {comp_size:.1f}MB ({ratio:.0f}% reduction)")
                    return
            except (FileNotFoundError, IOError):
                pass  # Fall back to normal handling

        # Call parent implementation for other files
        result = super().do_GET()
        return result

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('positional_port', nargs='?', type=int, default=8000)
    parser.add_argument('positional_host', nargs='?', default='')
    parser.add_argument('--port', type=int)
    parser.add_argument('--host')
    args = parser.parse_args()
    PORT = args.port if args.port is not None else args.positional_port
    HOST = args.host if args.host is not None else args.positional_host

    # Allow socket reuse
    socketserver.TCPServer.allow_reuse_address = True

    # Use ThreadingTCPServer for better compatibility
    class ThreadedTCPServer(socketserver.ThreadingMixIn, socketserver.TCPServer):
        pass

    with ThreadedTCPServer((HOST, PORT), CORSRequestHandler) as httpd:
        import socket
        hostname = socket.gethostname()
        local_ip = socket.gethostbyname(hostname)

        print(f"\n{'='*60}")
        print(f"Zcash Wallet WASM Development Server")
        print(f"{'='*60}")
        display_host = HOST if HOST else "0.0.0.0"
        print(f"Listening on: {display_host}:{PORT}")
        print(f"\nAccess URLs:")
        print(f"  Local:      http://localhost:{PORT}/")
        print(f"  Local IPv4: http://127.0.0.1:{PORT}/")
        print(f"  Network:    http://{local_ip}:{PORT}/")
        print(f"\nAvailable test pages:")
        print(f"  - test_wallet.html (Wallet functionality)")
        print(f"  - test_message_signing.html (Message signing)")
        print(f"  - test_encryption.html (Encryption/Decryption)")
        print(f"  - test_mnemonic.html (Mnemonic tools)")
        print(f"\nPress Ctrl+C to stop the server")
        print(f"{'='*60}\n")

        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\n\nServer stopped.")
