import { describe, it, expect, vi, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useSurfaceVisibility } from './useSurfaceVisibility';

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state,
  });
  document.dispatchEvent(new Event('visibilitychange'));
}

describe('useSurfaceVisibility', () => {
  afterEach(() => setVisibility('visible'));

  it('starts visible when the document is visible', () => {
    setVisibility('visible');
    const { result } = renderHook(() => useSurfaceVisibility());
    expect(result.current).toBe(true);
  });

  it('flips to false when the document becomes hidden', () => {
    const { result } = renderHook(() => useSurfaceVisibility());
    act(() => setVisibility('hidden'));
    expect(result.current).toBe(false);
  });

  it('returns to true when the document comes back', () => {
    const { result } = renderHook(() => useSurfaceVisibility());
    act(() => setVisibility('hidden'));
    act(() => setVisibility('visible'));
    expect(result.current).toBe(true);
  });

  it('unsubscribes on unmount', () => {
    const remove = vi.spyOn(document, 'removeEventListener');
    const { unmount } = renderHook(() => useSurfaceVisibility());
    unmount();
    expect(remove).toHaveBeenCalledWith('visibilitychange', expect.any(Function));
  });
});
