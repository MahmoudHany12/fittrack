import { resolveApiUrl } from './client';

describe('resolveApiUrl', () => {
  it('prefers the explicit env URL and trims trailing slashes', () => {
    expect(resolveApiUrl('https://api.example.com/', '10.0.0.5:8081')).toBe('https://api.example.com');
  });
  it('falls back to the dev server host on port 8080', () => {
    expect(resolveApiUrl(undefined, '10.50.0.161:8081')).toBe('http://10.50.0.161:8080');
    expect(resolveApiUrl('  ', '192.168.1.22:8081')).toBe('http://192.168.1.22:8080');
  });
  it('uses localhost when nothing is known', () => {
    expect(resolveApiUrl(undefined, null)).toBe('http://localhost:8080');
  });
});
