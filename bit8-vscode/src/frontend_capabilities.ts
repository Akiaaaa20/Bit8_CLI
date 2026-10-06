export interface FrontendCapabilities {
  canRunNativeRuntime: boolean;
  canEditWorkspace: boolean;
  canUseCustomEditors: boolean;
}

/** Capabilities are supplied by a frontend adapter; project models do not inspect the host. */
export function frontendCapabilities(
  available: Partial<FrontendCapabilities> = {},
): Readonly<FrontendCapabilities> {
  return Object.freeze({
    canRunNativeRuntime: available.canRunNativeRuntime ?? false,
    canEditWorkspace: available.canEditWorkspace ?? false,
    canUseCustomEditors: available.canUseCustomEditors ?? false,
  });
}
