// Metro config: Expo defaults plus a development-only proxy so the phone reaches the API
// through the same address that serves the app (/api/* -> http://127.0.0.1:8080/*).
// This works on any Wi-Fi, through Windows' firewall rules for Metro, and with `expo start --tunnel`.
const http = require('http');
const { getDefaultConfig } = require('expo/metro-config');

const config = getDefaultConfig(__dirname);
const API_TARGET = { host: '127.0.0.1', port: Number(process.env.FITTRACK_API_PORT || 8080) };

config.server = config.server || {};
const previous = config.server.enhanceMiddleware;
config.server.enhanceMiddleware = (middleware, server) => {
  const inner = previous ? previous(middleware, server) : middleware;
  return (req, res, next) => {
    if (!req.url || !req.url.startsWith('/api/')) return inner(req, res, next);
    const upstream = http.request(
      {
        ...API_TARGET,
        method: req.method,
        path: req.url.slice('/api'.length),
        headers: { ...req.headers, host: `${API_TARGET.host}:${API_TARGET.port}` },
      },
      (up) => {
        res.writeHead(up.statusCode || 502, up.headers);
        up.pipe(res);
      },
    );
    upstream.on('error', () => {
      res.writeHead(502, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ error: { code: 'api_unreachable', message: 'The FitTrack server is not running on this computer.' } }));
    });
    req.pipe(upstream);
  };
};

module.exports = config;
