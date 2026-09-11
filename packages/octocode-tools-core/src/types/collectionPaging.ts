export type CollectionSurface =
  'changedFiles' | 'discussion' | 'inline' | 'reviews' | 'commits';

export type CollectionPages = Partial<Record<CollectionSurface, number>>;

export type CollectionState = { page: number; hasMore: boolean };

export type CollectionStates = Partial<
  Record<CollectionSurface, CollectionState>
>;

export type CollectionArray<T> = T[] & { collectionState?: CollectionState };
