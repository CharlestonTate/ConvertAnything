import type { DragEvent } from "react";

const SKIP_NAMES = new Set([".ds_store", "thumbs.db", "desktop.ini"]);

export function normalizeExt(name: string): string | null {
  const raw = name.split(".").pop()?.toLowerCase();
  if (!raw || raw === name.toLowerCase()) return null;
  if (raw === "jpeg") return "jpg";
  if (raw === "tif") return "tiff";
  return raw;
}

function shouldSkip(file: File): boolean {
  const base = file.name.split(/[/\\]/).pop() ?? file.name;
  if (base.startsWith(".")) return true;
  return SKIP_NAMES.has(base.toLowerCase());
}

export async function filesFromDrop(event: DragEvent): Promise<File[]> {
  const items = event.dataTransfer?.items;
  if (items && items.length > 0 && typeof items[0].webkitGetAsEntry === "function") {
    const collected: File[] = [];
    const entries = [...items]
      .map((item) => item.webkitGetAsEntry())
      .filter((entry): entry is FileSystemEntry => Boolean(entry));
    for (const entry of entries) {
      await collectEntry(entry, collected);
    }
    if (collected.length > 0) return collected.filter((file) => !shouldSkip(file));
  }
  return [...(event.dataTransfer?.files ?? [])].filter((file) => !shouldSkip(file));
}

export function filesFromInput(list: FileList | null): File[] {
  if (!list) return [];
  return [...list].filter((file) => !shouldSkip(file));
}

async function collectEntry(entry: FileSystemEntry, out: File[]): Promise<void> {
  if (entry.isFile) {
    const file = await new Promise<File>((resolve, reject) => {
      (entry as FileSystemFileEntry).file(resolve, reject);
    });
    out.push(file);
    return;
  }
  if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    for (const child of await readAllEntries(reader)) {
      await collectEntry(child, out);
    }
  }
}

function readAllEntries(reader: FileSystemDirectoryReader): Promise<FileSystemEntry[]> {
  return new Promise((resolve, reject) => {
    const all: FileSystemEntry[] = [];
    const pump = () => {
      reader.readEntries((batch) => {
        if (batch.length === 0) {
          resolve(all);
          return;
        }
        all.push(...batch);
        pump();
      }, reject);
    };
    pump();
  });
}
