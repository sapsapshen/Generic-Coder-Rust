import type { ServiceIdentifier } from './serviceIdentifier';
import { ServiceCollection } from './serviceCollection';

export interface ServicesAccessor {
  get<T>(id: ServiceIdentifier<T>): T;
}

export class InstantiationService {
  constructor(readonly services: ServiceCollection) {}

  createInstance<T>(ctor: new (accessor: ServicesAccessor, ...args: any[]) => T, ...args: any[]): T {
    return new ctor(this, ...args);
  }

  get<T>(id: ServiceIdentifier<T>): T {
    return this.services.get(id);
  }
}
