export type AuthMode = "offline" | "microsoft";

export interface LauncherSession {
  id: string;
  username: string;
  mode: AuthMode;
}

export interface MicrosoftDeviceCode {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
  message: string;
}
