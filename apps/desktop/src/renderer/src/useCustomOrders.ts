import { useCallback, useState } from 'react';

/** Custom playlist orders, keyed by `<libraryId>/<playlistId>`. Only kept on this computer. */
type CustomOrders = Record<string, string[]>;

const STORAGE_KEY = 'customPlaylistOrders';

function load(): CustomOrders {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}');
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(
        (entry): entry is [string, string[]] =>
          Array.isArray(entry[1]) && entry[1].every((id) => typeof id === 'string'),
      ),
    );
  } catch {
    return {};
  }
}

export function useCustomOrders() {
  const [orders, setOrders] = useState<CustomOrders>(load);

  const update = useCallback((change: (orders: CustomOrders) => CustomOrders) => {
    setOrders((previous) => {
      const next = change(previous);
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const saveOrder = useCallback(
    (key: string, trackIds: string[]) => update((previous) => ({ ...previous, [key]: trackIds })),
    [update],
  );

  const resetOrder = useCallback(
    (key: string) =>
      update((previous) => {
        const { [key]: _removed, ...rest } = previous;
        return rest;
      }),
    [update],
  );

  return { orders, saveOrder, resetOrder };
}
