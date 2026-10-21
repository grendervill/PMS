import { describe, expect, it } from "vitest";
import { totpUri } from "./otpauth";

describe("totpUri", () => {
  it("encodes issuer, account and non-default parameters", () => {
    const uri = totpUri({
      secret: "JBSWY3DPEHPK3PXP",
      algorithm: "SHA256",
      digits: 8,
      period: 60,
      issuer: "GitHub",
      account: "alex",
    });

    expect(uri).toContain("otpauth://totp/GitHub%3Aalex?");
    expect(uri).toContain("secret=JBSWY3DPEHPK3PXP");
    expect(uri).toContain("algorithm=SHA256");
    expect(uri).toContain("digits=8");
    expect(uri).toContain("period=60");
    expect(uri).toContain("issuer=GitHub");
  });
});
