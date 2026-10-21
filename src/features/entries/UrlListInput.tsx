import { Plus, Trash2 } from "lucide-react";
import { Field } from "@/components/ui/Field";
import { IconButton } from "@/components/ui/IconButton";
import { Input } from "@/components/ui/Input";

export function UrlListInput({
  urls,
  onChange,
}: {
  urls: string[];
  onChange: (urls: string[]) => void;
}) {
  const rows = urls.length > 0 ? urls : [""];

  const update = (index: number, value: string) => {
    onChange(rows.map((url, current) => (current === index ? value : url)));
  };

  const remove = (index: number) => {
    const next = rows.filter((_, current) => current !== index);
    onChange(next.length > 0 ? next : [""]);
  };

  return (
    <Field
      label="URL"
      htmlFor="entry-url-0"
      hint="Можно добавить несколько ссылок: первая будет основной."
    >
      <div className="flex items-start gap-2">
        <div className="grid min-w-0 flex-1 gap-2">
          {rows.map((url, index) => (
            <div key={index} className="flex min-w-0 items-center gap-1.5">
              <Input
                id={index === 0 ? "entry-url-0" : undefined}
                aria-label={index === 0 ? undefined : `Дополнительный URL ${index + 1}`}
                value={url}
                onChange={(event) => update(index, event.target.value)}
                placeholder={index === 0 ? "https://example.com" : "Ещё одна ссылка"}
                inputMode="url"
              />
              {rows.length > 1 || url ? (
                <IconButton
                  label={index === 0 ? "Удалить URL" : `Удалить URL ${index + 1}`}
                  size="sm"
                  onClick={() => remove(index)}
                >
                  <Trash2 size={14} />
                </IconButton>
              ) : null}
            </div>
          ))}
        </div>
        <IconButton
          label="Добавить URL"
          className="mt-0.5 shrink-0"
          onClick={() => onChange([...rows, ""])}
        >
          <Plus size={16} />
        </IconButton>
      </div>
    </Field>
  );
}
