import { useCallback, useState } from "react";

export function useAsyncAction(onError: (reason: unknown) => void) {
  const [pending, setPending] = useState<string | null>(null);

  const run = useCallback(
    async (key: string, action: () => Promise<void>) => {
      if (pending) return;
      setPending(key);
      try {
        await action();
      } catch (reason) {
        onError(reason);
      } finally {
        setPending(null);
      }
    },
    [onError, pending],
  );

  return { pending, run };
}
