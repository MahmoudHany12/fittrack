import { File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';

import { API_URL, ApiError, refreshAccessToken } from './client';
import { loadTokens } from './tokens';

export type CsvTable = 'workouts' | 'sets' | 'food' | 'sleep' | 'weight';
export const CSV_TABLES: readonly CsvTable[] = ['workouts', 'sets', 'food', 'sleep', 'weight'];

export type ExportRequest = { format: 'json' } | { format: 'csv'; table: CsvTable };

export function exportPath(req: ExportRequest): string {
  return req.format === 'json' ? '/export?format=json' : `/export?format=csv&table=${req.table}`;
}

export function exportFileName(req: ExportRequest): string {
  return req.format === 'json' ? 'fittrack-export.json' : `fittrack-${req.table}.csv`;
}

async function fetchText(path: string, accessToken: string | undefined): Promise<Response> {
  try {
    return await fetch(`${API_URL}${path}`, {
      headers: accessToken ? { Authorization: `Bearer ${accessToken}` } : {},
    });
  } catch {
    throw new ApiError(0, 'network', 'Cannot reach the server. Check your connection.');
  }
}

/** Authenticated GET returning the raw body (the export is not always JSON). Refreshes once on 401. */
export async function downloadExport(req: ExportRequest): Promise<string> {
  const path = exportPath(req);
  const tokens = await loadTokens();
  let res = await fetchText(path, tokens?.accessToken);
  if (res.status === 401) {
    const fresh = await refreshAccessToken();
    if (!fresh) throw new ApiError(401, 'unauthorized', 'Your session has ended. Please sign in again.');
    res = await fetchText(path, fresh);
  }
  if (!res.ok) {
    throw new ApiError(res.status, 'error', 'Could not export your data. Please try again.');
  }
  return res.text();
}

/** Download the export, save it in the cache and open the OS share sheet. */
export async function shareExport(req: ExportRequest): Promise<void> {
  if (!(await Sharing.isAvailableAsync())) {
    throw new Error('Sharing is not available on this device.');
  }
  const body = await downloadExport(req);
  const file = new File(Paths.cache, exportFileName(req));
  file.create({ overwrite: true });
  file.write(body);
  await Sharing.shareAsync(file.uri, {
    mimeType: req.format === 'json' ? 'application/json' : 'text/csv',
    dialogTitle: 'Export your FitTrack data',
    UTI: req.format === 'json' ? 'public.json' : 'public.comma-separated-values-text',
  });
}
