import { resolveApiUrl } from './client';

describe('resolveApiUrl', () => {
  it('prefers the explicit env URL and trims trailing slashes', () => {
    expect(resolveApiUrl('https://api.example.com/', '10.0.0.5:8081')).toBe('https://api.example.com');
  });
  it('goes through the dev server proxy when no env URL is set', () => {
    expect(resolveApiUrl(undefined, '10.50.0.161:8081')).toBe('http://10.50.0.161:8081/api');
    expect(resolveApiUrl('  ', 'abc-anonymous-8081.exp.direct')).toBe('https://abc-anonymous-8081.exp.direct/api');
  });
  it('uses localhost when nothing is known', () => {
    expect(resolveApiUrl(undefined, null)).toBe('http://localhost:8080');
  });
});
