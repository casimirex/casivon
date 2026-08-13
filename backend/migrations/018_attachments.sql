-- Uploaded files, and the first thing to hang one off: an expense line.
--
-- `expense_lines.receipt_url` has been in the schema since `007_create_hr.sql`
-- and is carried faithfully through the entity, the DTO and the repository —
-- but nothing in the product can set it. There is no upload endpoint, and the
-- expense form never rendered an input for it. So an approver deciding on a
-- claim has never had anything to check the amount against.
--
-- Everything a download needs beyond the bytes lives here rather than in the
-- object store: the name the file arrived under, what it actually turned out to
-- be, and who put it there. Those cannot be recovered from a storage key, which
-- is deliberately a generated uuid with nothing of the client's in it.

CREATE TABLE attachments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    -- Where the bytes are in the bucket: `receipts/{yyyy}/{mm}/{uuid}.{ext}`.
    -- Generated server-side and never derived from the upload, so a filename
    -- containing `../` or a leading slash cannot address anything it should
    -- not. UNIQUE because two rows pointing at one object would make deletion
    -- ambiguous.
    storage_key TEXT NOT NULL UNIQUE,

    -- The name the user's file had, kept only to display and to name the
    -- download. Sanitised on the way in, because it is interpolated into a
    -- Content-Disposition header.
    file_name TEXT NOT NULL,

    -- What the *server* determined from the leading bytes, never what the
    -- client claimed. This value is handed back to a browser as the type to
    -- render, so trusting the upload here would let somebody store a script and
    -- have it served under our origin.
    content_type TEXT NOT NULL,

    byte_size BIGINT NOT NULL,

    -- Who uploaded it. This is also the authorization fallback: an attachment
    -- not yet attached to anything — the window between choosing a file and
    -- saving the form — is readable by this user and nobody else.
    uploaded_by UUID NOT NULL REFERENCES users(id),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_attachments_uploaded_by ON attachments(uploaded_by);

-- The receipt itself. Per line rather than per report so that an approver
-- looking at a single amount can see the document that justifies that amount,
-- which is the whole point of asking for one.
--
-- ON DELETE SET NULL: deleting a claim should not be blocked by a file, and the
-- attachment row surviving is what lets an orphan still be found later.
ALTER TABLE expense_lines
    ADD COLUMN receipt_attachment_id UUID REFERENCES attachments(id) ON DELETE SET NULL;

-- Reading an attachment walks *backwards* from the file to the claim it belongs
-- to, to work out who is allowed to see it. That lookup is on every download,
-- so it gets an index.
CREATE INDEX idx_expense_lines_receipt ON expense_lines(receipt_attachment_id);

-- `receipt_url` is deliberately left in place. Dropping a column cannot be
-- undone, it costs nothing to keep, and in practice it is empty everywhere
-- since no input was ever rendered for it. Nothing written from here on sets
-- it, and the UI no longer offers it.
