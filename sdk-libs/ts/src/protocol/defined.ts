/** `fields` without its undefined values, the spread an exact optional property takes. */
export function defined<Fields extends object>(fields: Fields): Defined<Fields> {
  return Object.fromEntries(
    Object.entries(fields).filter(([, value]) => value !== undefined),
  ) as Defined<Fields>;
}

type Defined<Fields> = { [Name in keyof Fields]?: Exclude<Fields[Name], undefined> };
