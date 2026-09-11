/** Canonical, side-effect-free Awareness discovery for tooling and hosts. */
export {
  AWARENESS_CONCEPTS,
  ROUTINE_AWARENESS_OPERATIONS,
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
} from './schema/operation-catalog.js';
export type {
  AwarenessConcept,
  AwarenessOperation,
  AwarenessOperationCall,
  AwarenessOperationDescriptor,
  AwarenessOperationParams,
} from './schema/operation-catalog.js';
