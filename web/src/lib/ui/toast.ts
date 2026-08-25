import { writable } from "svelte/store";

export type ToastKind = "success" | "error" | "info";
export type Toast = { id: number; kind: ToastKind; message: string };

const toasts = writable<Toast[]>([]);
let nextId = 1;

function push(kind: ToastKind, message: string): void {
  const id = nextId;
  nextId += 1;
  toasts.update((items) => [...items, { id, kind, message }]);
  setTimeout(() => toast.dismiss(id), kind === "error" ? 7000 : 4000);
}

export const toast = {
  subscribe: toasts.subscribe,
  success(message: string): void {
    push("success", message);
  },
  error(message: string): void {
    push("error", message);
  },
  info(message: string): void {
    push("info", message);
  },
  dismiss(id: number): void {
    toasts.update((items) => items.filter((item) => item.id !== id));
  },
};
