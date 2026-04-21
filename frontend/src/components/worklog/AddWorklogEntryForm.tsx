import { useState, useCallback, useRef } from 'react';

interface Props {
  readonly onSubmit: (body: string) => Promise<void>;
  readonly placeholder?: string;
}

export function AddWorklogEntryForm({ onSubmit, placeholder }: Props) {
  const [value, setValue] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const submit = useCallback(async () => {
    const trimmed = value.trim();
    if (!trimmed || submitting) return;
    setSubmitting(true);
    try {
      await onSubmit(trimmed);
      setValue('');
      textareaRef.current?.focus();
    } finally {
      setSubmitting(false);
    }
  }, [value, submitting, onSubmit]);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        void submit();
      }
    },
    [submit]
  );

  return (
    <div className="space-y-2">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder ?? 'Log an entry… (Ctrl+Enter to submit)'}
        rows={3}
        className="w-full rounded-md border border-gray-300 p-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        disabled={submitting}
      />
      <div className="flex justify-end">
        <button
          type="button"
          onClick={submit}
          disabled={!value.trim() || submitting}
          className="rounded-md bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-700 disabled:bg-gray-300"
        >
          {submitting ? 'Logging…' : 'Log entry'}
        </button>
      </div>
    </div>
  );
}
