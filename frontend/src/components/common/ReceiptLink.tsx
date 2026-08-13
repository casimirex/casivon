import { useState } from 'react';
import { FileText, ImageIcon, Loader2 } from 'lucide-react';
import { useFileLink } from '@/hooks/useFiles';

/**
 * Shows the receipt attached to a line, for whoever is allowed to see it.
 *
 * The link is fetched lazily and only for files the caller may read: the API
 * answers 404 for somebody else's, so a claim listing that includes an
 * attachment id still cannot be turned into a way of reading it.
 *
 * Images open in a preview, PDFs open in a new tab. Both use the presigned URL
 * directly, which is why no token is involved.
 */
export function ReceiptLink({ attachmentId }: { attachmentId: string }) {
  const [expanded, setExpanded] = useState(false);
  const link = useFileLink(attachmentId);

  if (link.isLoading) {
    return <Loader2 className="h-4 w-4 animate-spin text-slate-300" aria-label="Loading receipt" />;
  }

  // A receipt the caller may not read, or one whose file has gone. Neither is
  // worth an error banner in the middle of a claim — the amount is still
  // readable and that is what the row is about.
  if (link.error || !link.data) {
    return <span className="text-xs text-slate-400">—</span>;
  }

  const { url, file_name, is_image } = link.data;

  if (!is_image) {
    return (
      <a
        href={url}
        target="_blank"
        rel="noreferrer"
        className="inline-flex items-center gap-1 text-xs text-blue-600 hover:underline"
      >
        <FileText className="h-3.5 w-3.5" />
        {file_name}
      </a>
    );
  }

  return (
    <div className="space-y-1">
      <button
        type="button"
        onClick={() => setExpanded((open) => !open)}
        className="inline-flex items-center gap-1 text-xs text-blue-600 hover:underline"
      >
        <ImageIcon className="h-3.5 w-3.5" />
        {expanded ? 'Hide' : file_name}
      </button>
      {expanded && (
        <a href={url} target="_blank" rel="noreferrer" className="block">
          <img
            src={url}
            alt={file_name}
            className="max-h-64 rounded border border-slate-200 object-contain"
          />
        </a>
      )}
    </div>
  );
}
