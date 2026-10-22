/** Joins class names, skipping falsy values (`cx(styles.a, active && styles.b)`) */
export const cx = (...classes: Array<string | false | null | undefined>): string =>
  classes.filter(Boolean).join(' ');
