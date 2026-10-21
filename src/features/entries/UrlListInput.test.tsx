import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UrlListInput } from "./UrlListInput";

describe("UrlListInput", () => {
  it("edits, adds and removes url rows", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <UrlListInput urls={["https://a.example"]} onChange={onChange} />,
    );

    fireEvent.change(screen.getByLabelText("URL"), {
      target: { value: "https://edited.example" },
    });
    expect(onChange).toHaveBeenLastCalledWith(["https://edited.example"]);

    fireEvent.click(screen.getByRole("button", { name: "Добавить URL" }));
    expect(onChange).toHaveBeenLastCalledWith(["https://a.example", ""]);

    rerender(
      <UrlListInput urls={["https://a.example", ""]} onChange={onChange} />,
    );
    fireEvent.change(screen.getByLabelText("Дополнительный URL 2"), {
      target: { value: "https://b.example" },
    });
    expect(onChange).toHaveBeenLastCalledWith(["https://a.example", "https://b.example"]);

    fireEvent.click(screen.getByRole("button", { name: "Удалить URL" }));
    expect(onChange).toHaveBeenLastCalledWith([""]);
  });
});
