import type { DjUsbApi } from '../../shared/types';

declare global {
  interface Window {
    /** Exposed by the preload script. */
    djusb: DjUsbApi;
  }
}
