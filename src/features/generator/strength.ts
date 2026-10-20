import type { ZxcvbnFactory } from "@zxcvbn-ts/core";

export type StrengthScore = 0 | 1 | 2 | 3 | 4;

export type PasswordStrength = {
  score: StrengthScore;
  label: string;
};

const labels = ["Очень слабый", "Слабый", "Средний", "Надёжный", "Очень надёжный"];

let enginePromise: Promise<ZxcvbnFactory> | null = null;

async function getEngine(): Promise<ZxcvbnFactory> {
  if (!enginePromise) {
    enginePromise = Promise.all([
      import("@zxcvbn-ts/core"),
      import("@zxcvbn-ts/language-common"),
      import("@zxcvbn-ts/language-ru"),
    ]).then(([core, common, ru]) => {
      return new core.ZxcvbnFactory({
        dictionary: {
          ...common.dictionary,
          ...ru.dictionary,
        },
        graphs: common.adjacencyGraphs,
        translations: ru.translations,
      });
    });
  }
  return enginePromise;
}

export async function passwordStrength(password: string): Promise<PasswordStrength> {
  if (!password) {
    return { score: 0, label: labels[0] ?? "Очень слабый" };
  }

  const engine = await getEngine();
  const result = engine.check(password);
  const score = Math.max(0, Math.min(4, result.score)) as StrengthScore;
  return { score, label: labels[score] ?? labels[0] ?? "Очень слабый" };
}
