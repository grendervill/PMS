import { create } from "zustand";
import type { EntryDraft } from "@/features/entries/entries";

export type EditorDraftSnapshot = {
  id: string;
  payload: EntryDraft;
};

type EditorDraftStore = {
  snapshot: EditorDraftSnapshot | null;
  setSnapshot: (snapshot: EditorDraftSnapshot) => void;
  clear: () => void;
};

export const useEditorDraftStore = create<EditorDraftStore>((set) => ({
  snapshot: null,
  setSnapshot: (snapshot) => set({ snapshot }),
  clear: () => set({ snapshot: null }),
}));
