import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export async function copySecret(value: string): Promise<void> {
  if (!value) return;
  await writeText(value);
}
