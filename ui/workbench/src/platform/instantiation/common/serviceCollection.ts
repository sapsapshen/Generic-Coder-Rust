import type { ServiceIdentifier } from './serviceIdentifier';

export class ServiceCollection {
  private readonly entries = new Map<ServiceIdentifier<unknown>, unknown>();

  set<T>(id: ServiceIdentifier<T>, instance: T): T {
    this.entries.set(id, instance);
    return instance;
  }

  get<T>(id: ServiceIdentifier<T>): T {
    const value = this.entries.get(id);
    if (!value) {
      throw new Error(`Missing service: ${String(id.description || id.toString())}`);
    }
    return value as T;
  }
}
