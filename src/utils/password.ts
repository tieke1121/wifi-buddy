export interface PasswordStrength {
  level: "empty" | "weak" | "medium" | "strong";
  label: string;
  percent: number;
  barClass: string;
  textClass: string;
}

export function evaluatePasswordStrength(pw: string): PasswordStrength {
  if (!pw) {
    return {
      level: "empty",
      label: "未填写",
      percent: 0,
      barClass: "bg-slate-200 dark:bg-zinc-700",
      textClass: "text-slate-400",
    };
  }
  let score = 0;
  if (pw.length >= 8) score += 1;
  if (pw.length >= 12) score += 1;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) score += 1;
  if (/\d/.test(pw)) score += 1;
  if (/[^A-Za-z0-9]/.test(pw)) score += 1;

  if (pw.length < 8 || score <= 1) {
    return {
      level: "weak",
      label: pw.length < 8 ? "太短（至少需 8 位）" : "偏弱（易被猜出）",
      percent: 30,
      barClass: "bg-rose-500",
      textClass: "text-rose-500",
    };
  }
  if (score <= 3) {
    return {
      level: "medium",
      label: "一般（建议加大小写或符号）",
      percent: 65,
      barClass: "bg-amber-500",
      textClass: "text-amber-500",
    };
  }
  return {
    level: "strong",
    label: "强密码（很安全）",
    percent: 100,
    barClass: "bg-emerald-500",
    textClass: "text-emerald-600 dark:text-emerald-400",
  };
}

const WORDS = [
  "Sunny",
  "Cloud",
  "River",
  "Maple",
  "Ocean",
  "Breeze",
  "Cedar",
  "Spark",
  "Coral",
  "Panda",
  "Star",
  "Moon",
];

const SYMBOLS = ["@", "#", "!", "$"];

export function generateMemorablePassword(): string {
  const word = WORDS[Math.floor(Math.random() * WORDS.length)];
  const sym = SYMBOLS[Math.floor(Math.random() * SYMBOLS.length)];
  const digits = Math.floor(1000 + Math.random() * 9000);
  return `${word}${sym}${digits}`;
}
