import { useRef, useState } from 'react';
import { Loader2, Paperclip, X } from 'lucide-react';
import { ApiError } from '@/api/client';
import {
  ACCEPTED_FILE_TYPES,
  MAX_UPLOAD_BYTES,
  formatFileSize,
  useUploadFile,
} from '@/hooks/useFiles';

interface FileUploadProps {
  /** The attachment already on this record, if any. */
  value?: string | null;
  /** Called with the new attachment id, or `null` when it is taken off. */
  onChange: (attachmentId: string | null) => void;
  /** Shown next to the control once a file is attached. */
  fileName?: string;
  label?: string;
  disabled?: boolean;
}

/**
 * Picks a file and uploads it immediately, handing back the id.
 *
 * Uploading on selection rather than on submit is what lets the id exist before
 * the form is saved — the record being attached to may not exist yet. The cost
 * is a file in the bucket that nothing references if the form is abandoned;
 * those are readable only by whoever uploaded them, so they sit there inert.
 */
export function FileUpload({
  value,
  onChange,
  fileName,
  label = 'Receipt',
  disabled,
}: FileUploadProps) {
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState<string | null>(fileName ?? null);
  const upload = useUploadFile();

  function handlePick(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    // Clearing the input is what makes picking the same file twice fire a
    // change event again — after a failed upload, the obvious thing to try.
    event.target.value = '';
    if (!file) return;

    setError(null);

    // Checked here as well as on the server so the user is told before ten
    // megabytes go up the wire, not after.
    if (file.size > MAX_UPLOAD_BYTES) {
      setError(
        `That file is ${formatFileSize(file.size)}. The limit is ${formatFileSize(MAX_UPLOAD_BYTES)}.`
      );
      return;
    }

    upload.mutate(file, {
      onSuccess: (attachment) => {
        setName(attachment.file_name);
        onChange(attachment.id);
      },
      onError: (cause) => {
        setError(cause instanceof ApiError ? cause.message : 'The upload failed.');
      },
    });
  }

  function handleRemove() {
    setName(null);
    setError(null);
    // Detaches rather than deletes: the file stays in the bucket, unreferenced
    // and readable only by whoever put it there.
    onChange(null);
  }

  return (
    <div className="space-y-1">
      <input
        ref={input}
        type="file"
        className="hidden"
        accept={ACCEPTED_FILE_TYPES}
        onChange={handlePick}
        aria-label={label}
        data-testid="file-input"
      />

      {value ? (
        <div className="flex items-center gap-2 rounded-md border border-slate-200 bg-slate-50 px-2 py-1.5 text-xs">
          <Paperclip className="h-3.5 w-3.5 shrink-0 text-slate-400" />
          <span className="min-w-0 flex-1 truncate text-slate-700">{name ?? 'Attached'}</span>
          <button
            type="button"
            onClick={handleRemove}
            disabled={disabled}
            className="rounded p-0.5 text-slate-400 hover:bg-slate-200 hover:text-slate-600"
            aria-label={`Remove ${name ?? 'attachment'}`}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => input.current?.click()}
          disabled={disabled || upload.isPending}
          className="flex w-full items-center justify-center gap-1.5 rounded-md border border-dashed border-slate-300 px-2 py-1.5 text-xs text-slate-500 hover:border-slate-400 hover:text-slate-700 disabled:opacity-60"
        >
          {upload.isPending ? (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              Uploading…
            </>
          ) : (
            <>
              <Paperclip className="h-3.5 w-3.5" />
              {label}
            </>
          )}
        </button>
      )}

      {error && (
        <p className="text-xs text-red-600" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
