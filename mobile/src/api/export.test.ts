import { File } from 'expo-file-system';
import * as Sharing from 'expo-sharing';

import { refreshAccessToken } from './client';
import { downloadExport, exportFileName, exportPath, shareExport } from './export';
import { loadTokens } from './tokens';

jest.mock('./client', () => ({
  API_URL: 'http://api.test',
  ApiError: class ApiError extends Error {
    status: number;
    code: string;
    constructor(status: number, code: string, message: string) {
      super(message);
      this.status = status;
      this.code = code;
    }
  },
  refreshAccessToken: jest.fn(),
}));
jest.mock('./tokens', () => ({ loadTokens: jest.fn() }));
jest.mock(
  'expo-file-system',
  () => {
    const write = jest.fn();
    const create = jest.fn();
    return {
      Paths: { cache: 'cache://' },
      File: jest.fn().mockImplementation((_dir: string, name: string) => ({
        uri: `file:///cache/${name}`,
        create,
        write,
      })),
    };
  },
  { virtual: true },
);
jest.mock(
  'expo-sharing',
  () => ({ isAvailableAsync: jest.fn(), shareAsync: jest.fn() }),
  { virtual: true },
);

const mockFetch = jest.fn();
beforeEach(() => {
  mockFetch.mockReset();
  (globalThis as { fetch?: unknown }).fetch = mockFetch;
  (loadTokens as jest.Mock).mockResolvedValue({ accessToken: 'tok', refreshToken: 'r' });
  (refreshAccessToken as jest.Mock).mockReset();
  (Sharing.isAvailableAsync as jest.Mock).mockResolvedValue(true);
  (Sharing.shareAsync as jest.Mock).mockReset();
  (File as unknown as jest.Mock).mockClear();
});

const ok = (text: string) => ({ ok: true, status: 200, text: async () => text });

describe('export paths', () => {
  it('builds urls and file names', () => {
    expect(exportPath({ format: 'json' })).toBe('/export?format=json');
    expect(exportPath({ format: 'csv', table: 'sets' })).toBe('/export?format=csv&table=sets');
    expect(exportFileName({ format: 'json' })).toBe('fittrack-export.json');
    expect(exportFileName({ format: 'csv', table: 'food' })).toBe('fittrack-food.csv');
  });
});

describe('downloadExport', () => {
  it('sends the bearer token and returns the body', async () => {
    mockFetch.mockResolvedValue(ok('a,b'));
    await expect(downloadExport({ format: 'csv', table: 'weight' })).resolves.toBe('a,b');
    expect(mockFetch).toHaveBeenCalledWith('http://api.test/export?format=csv&table=weight', {
      headers: { Authorization: 'Bearer tok' },
    });
  });

  it('refreshes once on 401 then retries', async () => {
    mockFetch.mockResolvedValueOnce({ ok: false, status: 401 }).mockResolvedValueOnce(ok('{}'));
    (refreshAccessToken as jest.Mock).mockResolvedValue('fresh');
    await expect(downloadExport({ format: 'json' })).resolves.toBe('{}');
    expect(mockFetch).toHaveBeenLastCalledWith('http://api.test/export?format=json', {
      headers: { Authorization: 'Bearer fresh' },
    });
  });

  it('fails when the session cannot be refreshed or the server errors', async () => {
    mockFetch.mockResolvedValue({ ok: false, status: 401 });
    (refreshAccessToken as jest.Mock).mockResolvedValue(null);
    await expect(downloadExport({ format: 'json' })).rejects.toMatchObject({ status: 401 });
    mockFetch.mockResolvedValue({ ok: false, status: 500 });
    await expect(downloadExport({ format: 'json' })).rejects.toMatchObject({ status: 500 });
  });

  it('reports network errors', async () => {
    mockFetch.mockRejectedValue(new Error('offline'));
    await expect(downloadExport({ format: 'json' })).rejects.toMatchObject({ code: 'network' });
  });
});

describe('shareExport', () => {
  it('writes the file and opens the share sheet', async () => {
    mockFetch.mockResolvedValue(ok('x'));
    await shareExport({ format: 'csv', table: 'workouts' });
    const file = (File as unknown as jest.Mock).mock.results[0]!.value;
    expect(file.create).toHaveBeenCalledWith({ overwrite: true });
    expect(file.write).toHaveBeenCalledWith('x');
    expect(Sharing.shareAsync).toHaveBeenCalledWith(
      'file:///cache/fittrack-workouts.csv',
      expect.objectContaining({ mimeType: 'text/csv' }),
    );
  });

  it('does not download when sharing is unavailable', async () => {
    (Sharing.isAvailableAsync as jest.Mock).mockResolvedValue(false);
    await expect(shareExport({ format: 'json' })).rejects.toThrow('Sharing is not available');
    expect(mockFetch).not.toHaveBeenCalled();
  });
});
