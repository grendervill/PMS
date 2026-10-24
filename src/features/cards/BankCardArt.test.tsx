import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createBankCard, emptyCardDraft } from "./cardModel";
import { BankCardArt } from "./BankCardArt";

const card = createBankCard(
  {
    ...emptyCardDraft(),
    number: "4111111111111111",
    expiry: "12/29",
    cvv: "123",
    holder: "IVAN IVANOV",
    purpose: "Основная",
    imagePreset: "midnight",
  },
  "2026-10-24T10:00:00.000Z",
);

describe("BankCardArt", () => {
  it("shows the front with purpose, masked number and holder", () => {
    const { container } = render(
      <BankCardArt
        card={card}
        imageUrl={null}
        revealNumber={false}
        flipped={false}
      />,
    );

    expect(screen.getByText("Основная")).toBeInTheDocument();
    expect(screen.getByText("IVAN IVANOV")).toBeInTheDocument();
    expect(screen.getByText("•••• •••• •••• 1111")).toBeInTheDocument();
    expect(container.querySelector(".pms-card-art")).toHaveAttribute(
      "data-preset",
      "midnight",
    );
    expect(container.querySelector(".pms-card-art")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
  });

  it("reveals the full number when asked", () => {
    render(
      <BankCardArt card={card} imageUrl={null} revealNumber flipped={false} />,
    );
    expect(screen.getByText("4111 1111 1111 1111")).toBeInTheDocument();
  });

  it("marks the card as flipped and shows expiry and CVV on the back", () => {
    const { container } = render(
      <BankCardArt card={card} imageUrl={null} revealNumber flipped />,
    );

    expect(container.querySelector(".pms-card-art")).toHaveClass(
      "pms-card-art--flipped",
    );
    expect(screen.getByText("12/29")).toBeInTheDocument();
    expect(screen.getByText("123")).toBeInTheDocument();
    expect(screen.getByText("Срок")).toBeInTheDocument();
    expect(screen.getByText("CVV")).toBeInTheDocument();
  });

  it("uses a custom image when provided", () => {
    const { container } = render(
      <BankCardArt
        card={card}
        imageUrl="blob:card-image"
        revealNumber={false}
        flipped={false}
      />,
    );

    expect(container.querySelector(".pms-card-art")).toHaveAttribute(
      "data-custom",
      "true",
    );
    expect(container.querySelector(".pms-card-art__front")).toHaveStyle({
      backgroundImage: 'url("blob:card-image")',
    });
  });
});
