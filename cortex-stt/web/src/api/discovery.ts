import { post } from "./client";

export interface AnnounceResponse {
	host: string;
	port: number;
	uuid?: string;
}

/** Send a Supervisor /discovery announce. Used by the "Re-announce" button. */
export function announceDiscovery(): Promise<AnnounceResponse> {
	return post<AnnounceResponse>("/api/discovery/announce");
}
