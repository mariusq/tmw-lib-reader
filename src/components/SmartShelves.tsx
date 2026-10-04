import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ShelfFilter = {
  libraryRootId?: number | null;
  tagId?: number | null;
  collectionId?: number | null;
  readingStatus?: string | null;
  needsMetadata: boolean;
  hideDuplicateTitles: boolean;
  query: string;
  sort: string;
  offset: number;
  limit: number;
};
type Shelf = {
  id: number | null;
  name: string;
  version: number;
  filter: ShelfFilter;
  sortOrder: number;
};

export function SmartShelves({
  filter,
  onSelect,
}: {
  filter: ShelfFilter;
  onSelect: (filter: ShelfFilter) => void;
}) {
  const [shelves, setShelves] = useState<Shelf[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [order, setOrder] = useState(0);
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let current = true;
    void invoke<Shelf[]>("list_smart_shelves")
      .then((rows) => {
        if (current) setShelves(rows);
      })
      .catch((reason) => {
        if (current) setError(String(reason));
      });
    return () => {
      current = false;
    };
  }, []);
  const save = async (id: number | null) => {
    setSaving(true);
    setError(undefined);
    try {
      const savedId = await invoke<number>("save_smart_shelf", {
        shelf: { id, name, version: 1, filter, sortOrder: order },
      });
      setShelves(await invoke("list_smart_shelves"));
      setSelected(savedId);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };
  return (
    <section aria-label="Smart shelves" className="mb-5 rounded-lg border border-white/10 p-3">
      <p className="mb-2 text-sm text-stone-400">
        Smart shelves · save the current filters and sort order
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <select
          className="control max-w-xs"
          aria-label="Smart shelf"
          value={selected ?? ""}
          onChange={(event) => {
            const shelf = shelves.find((row) => row.id === Number(event.target.value));
            setSelected(shelf?.id ?? null);
            setName(shelf?.name ?? "");
            setOrder(shelf?.sortOrder ?? 0);
            if (shelf) onSelect(shelf.filter);
          }}
        >
          <option value="">New shelf</option>
          {shelves.map((shelf) => (
            <option key={shelf.id} value={shelf.id!}>
              {shelf.name}
            </option>
          ))}
        </select>
        <input
          aria-label="Shelf name"
          className="control max-w-xs"
          maxLength={120}
          placeholder="Shelf name"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <label className="text-sm">
          Position{" "}
          <input
            aria-label="Shelf position"
            type="number"
            className="control w-20"
            value={order}
            onChange={(event) => setOrder(Number(event.target.value))}
          />
        </label>
        <button
          disabled={saving || !name.trim()}
          className="primary"
          onClick={() => void save(null)}
        >
          Save new shelf
        </button>
        {selected !== null && (
          <>
            <button
              disabled={saving || !name.trim()}
              className="text-amber-300 underline"
              onClick={() => void save(selected)}
            >
              Update shelf
            </button>
            <button
              disabled={saving}
              className="text-stone-300 underline"
              onClick={async () => {
                setSaving(true);
                try {
                  await invoke("delete_smart_shelf", { shelfId: selected });
                  setShelves(await invoke("list_smart_shelves"));
                  setSelected(null);
                  setName("");
                } catch (reason) {
                  setError(String(reason));
                } finally {
                  setSaving(false);
                }
              }}
            >
              Delete shelf
            </button>
          </>
        )}
      </div>
      {error && (
        <p role="alert" className="mt-2 text-red-300">
          {error}
        </p>
      )}
    </section>
  );
}
