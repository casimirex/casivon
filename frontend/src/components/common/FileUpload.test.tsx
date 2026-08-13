import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { renderWithProviders } from '@/test/renderWithProviders';
import { FileUpload } from './FileUpload';
import { ReceiptLink } from './ReceiptLink';
import { ApiError, http } from '@/api/client';
import { MAX_UPLOAD_BYTES } from '@/hooks/useFiles';
import type { AttachmentLink, AttachmentSummary } from '@/types';

const UPLOADED: AttachmentSummary = {
  id: 'att-1',
  file_name: 'Taxi receipt.png',
  content_type: 'image/png',
  byte_size: 4096,
  created_at: '2026-03-02T09:00:00Z',
};

function png(name = 'Taxi receipt.png', size = 4096) {
  const file = new File([new Uint8Array(8)], name, { type: 'image/png' });
  // `File` size is derived from its contents, and building a ten-megabyte one
  // in a test would be wasteful; override it instead.
  Object.defineProperty(file, 'size', { value: size });
  return file;
}

/** The control is uncontrolled from the test's point of view otherwise: it
 *  reports an id upwards and expects that id to come back as `value`. */
function Harness({ onChange }: { onChange?: (id: string | null) => void }) {
  const [value, setValue] = useState<string | null>(null);
  return (
    <FileUpload
      value={value}
      onChange={(id) => {
        setValue(id);
        onChange?.(id);
      }}
    />
  );
}

describe('<FileUpload />', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('uploads on selection and reports the id back', async () => {
    const upload = vi.spyOn(http, 'upload').mockResolvedValue(UPLOADED as never);
    const onChange = vi.fn();
    renderWithProviders(<Harness onChange={onChange} />);

    await userEvent.upload(screen.getByTestId('file-input'), png());

    // The id has to exist before the form is saved, because the claim it will
    // hang off does not exist yet.
    await waitFor(() => expect(onChange).toHaveBeenCalledWith('att-1'));
    expect(upload).toHaveBeenCalledWith('/files', expect.any(File));
    expect(await screen.findByText('Taxi receipt.png')).toBeInTheDocument();
  });

  it('refuses an oversized file without asking the server', async () => {
    const upload = vi.spyOn(http, 'upload').mockResolvedValue(UPLOADED as never);
    renderWithProviders(<Harness />);

    await userEvent.upload(
      screen.getByTestId('file-input'),
      png('enormous.png', MAX_UPLOAD_BYTES + 1)
    );

    expect(await screen.findByRole('alert')).toHaveTextContent('The limit is 10.0 MB');
    // The whole point of checking here: ten megabytes never left the machine.
    expect(upload).not.toHaveBeenCalled();
  });

  it('shows what the server said when it refuses', async () => {
    vi.spyOn(http, 'upload').mockRejectedValue(
      new ApiError('That file does not look like JPEG, PNG, WebP or PDF.', 422)
    );
    renderWithProviders(<Harness />);

    await userEvent.upload(screen.getByTestId('file-input'), png());

    expect(await screen.findByRole('alert')).toHaveTextContent('does not look like');
  });

  it('detaches without deleting', async () => {
    vi.spyOn(http, 'upload').mockResolvedValue(UPLOADED as never);
    const onChange = vi.fn();
    renderWithProviders(<Harness onChange={onChange} />);

    await userEvent.upload(screen.getByTestId('file-input'), png());
    await screen.findByText('Taxi receipt.png');

    await userEvent.click(screen.getByRole('button', { name: /Remove Taxi receipt/ }));

    expect(onChange).toHaveBeenLastCalledWith(null);
    expect(screen.queryByText('Taxi receipt.png')).not.toBeInTheDocument();
  });
});

const LINK: AttachmentLink = {
  url: 'http://object-store.test/receipts/2026/03/abc.png?X-Amz-Signature=x',
  file_name: 'Taxi receipt.png',
  content_type: 'image/png',
  byte_size: 4096,
  expires_at: '2026-03-02T09:15:00Z',
  is_image: true,
};

describe('<ReceiptLink />', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('shows an image behind a toggle, pointing straight at the store', async () => {
    vi.spyOn(http, 'get').mockResolvedValue(LINK as never);
    renderWithProviders(<ReceiptLink attachmentId="att-1" />);

    await userEvent.click(await screen.findByRole('button', { name: /Taxi receipt/ }));

    // The presigned URL, not an API route: an <img> cannot send a bearer token.
    expect(screen.getByRole('img', { name: 'Taxi receipt.png' })).toHaveAttribute('src', LINK.url);
  });

  it('links a PDF out rather than trying to render it', async () => {
    vi.spyOn(http, 'get').mockResolvedValue({
      ...LINK,
      file_name: 'hotel.pdf',
      content_type: 'application/pdf',
      is_image: false,
    } as never);
    renderWithProviders(<ReceiptLink attachmentId="att-1" />);

    const link = await screen.findByRole('link', { name: 'hotel.pdf' });
    expect(link).toHaveAttribute('href', LINK.url);
    expect(link).toHaveAttribute('target', '_blank');
  });

  it('stays quiet about a receipt the caller may not read', async () => {
    // The API answers 404 for somebody else's file, so an attachment id in a
    // payload cannot be turned into a way of reading it. A row that says "—"
    // is the right amount of noise: the amount is still readable, which is what
    // the row is about.
    vi.spyOn(http, 'get').mockRejectedValue(new ApiError('File not found', 404));
    renderWithProviders(<ReceiptLink attachmentId="att-1" />);

    expect(await screen.findByText('—')).toBeInTheDocument();
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });
});
