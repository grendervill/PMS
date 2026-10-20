import { Eye, EyeOff } from "lucide-react";
import { forwardRef, useState, type InputHTMLAttributes } from "react";
import { IconButton } from "@/components/ui/IconButton";
import { Input } from "@/components/ui/Input";

export type SecretInputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type">;

export const SecretInput = forwardRef<HTMLInputElement, SecretInputProps>(
  ({ className, ...props }, ref) => {
    const [visible, setVisible] = useState(false);

    return (
      <div className="relative">
        <Input
          ref={ref}
          type={visible ? "text" : "password"}
          className={`pr-10 ${className ?? ""}`}
          autoComplete="off"
          autoCapitalize="off"
          spellCheck={false}
          {...props}
        />
        <IconButton
          label={visible ? "Скрыть" : "Показать"}
          size="sm"
          className="absolute right-1 top-1/2 -translate-y-1/2"
          onClick={() => setVisible((current) => !current)}
        >
          {visible ? <EyeOff size={15} /> : <Eye size={15} />}
        </IconButton>
      </div>
    );
  },
);

SecretInput.displayName = "SecretInput";
