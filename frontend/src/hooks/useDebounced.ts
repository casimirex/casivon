import { useEffect, useState } from 'react';

/**
 * The value, but only once it has stopped changing for `delay` milliseconds.
 *
 * Search fires on every keystroke, and without this a five-letter term is five
 * queries across fifteen tables, four of whose answers are thrown away — and
 * they can arrive out of order, so the discarded ones are not even harmless.
 */
export function useDebounced<T>(value: T, delay = 200): T {
  const [settled, setSettled] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), delay);
    // Cleanup runs on every change, so the timer only fires once typing pauses.
    return () => clearTimeout(timer);
  }, [value, delay]);

  return settled;
}
