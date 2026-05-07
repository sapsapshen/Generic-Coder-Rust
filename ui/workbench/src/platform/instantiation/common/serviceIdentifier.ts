export type ServiceIdentifier<T> = symbol & { readonly _serviceBrand: T };

export function createServiceIdentifier<T>(name: string): ServiceIdentifier<T> {
  return Symbol(name) as ServiceIdentifier<T>;
}
