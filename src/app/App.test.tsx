import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("renders the welcome screen", () => {
    render(<App />);
    expect(screen.getByText("Добро пожаловать")).toBeInTheDocument();
    expect(screen.getAllByText("PMS").length).toBeGreaterThan(0);
  });
});
