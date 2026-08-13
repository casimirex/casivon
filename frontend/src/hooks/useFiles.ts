import { useMutation, useQuery } from '@tanstack/react-query';
import { http } from '@/api/client';
import type { AttachmentLink, AttachmentSummary } from '@/types';

/** Mirrors `MAX_UPLOAD_BYTES` on the server, so the refusal arrives before the
 *  upload does rather than after ten megabytes have crossed the wire. */
export const MAX_UPLOAD_BYTES = 10 * 1024 * 1024;

/** Mirrors the server's byte-sniffing list. Only a hint to the file picker —
 *  the server decides, and it decides from the bytes, not from this. */
export const ACCEPTED_FILE_TYPES = 'image/jpeg,image/png,image/webp,application/pdf';

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function useUploadFile() {
  return useMutation({
    mutationFn: (file: File) => http.upload<AttachmentSummary>('/files', file),
  });
}

/**
 * A short-lived link to a stored file.
 *
 * The API answers with a presigned URL rather than the bytes, so the browser
 * fetches the file straight from object storage — which is also what makes an
 * `<img src>` work at all, since the session here is a bearer token in a header
 * and an image tag cannot send one.
 *
 * The link expires, so this deliberately does not cache for long: a URL held in
 * a stale query would start 403-ing from the object store, which looks like a
 * missing receipt rather than an expired link.
 */
export function useFileLink(id: string | null | undefined) {
  return useQuery({
    queryKey: ['file', id],
    queryFn: () => http.get<AttachmentLink>(`/files/${id}`),
    enabled: Boolean(id),
    staleTime: 5 * 60_000,
    gcTime: 5 * 60_000,
  });
}
