import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { setApiKey } from "@/api/client";
import { Activity } from "lucide-react";
import { type FormEvent, useState } from "react";

interface LoginPageProps {
	onLogin: () => void;
}

export function LoginPage({ onLogin }: LoginPageProps) {
	const [key, setKey] = useState("");
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);

	async function handleSubmit(e: FormEvent) {
		e.preventDefault();
		setError(null);

		const trimmed = key.trim();
		if (!trimmed) {
			setError("API key is required");
			return;
		}

		setLoading(true);
		try {
			const response = await fetch("/api/engine", {
				method: "GET",
				headers: {
					"Content-Type": "application/json",
					Authorization: `Bearer ${trimmed}`,
				},
			});

			if (response.ok) {
				setApiKey(trimmed);
				onLogin();
			} else if (response.status === 401 || response.status === 403) {
				setError("Invalid API key");
			} else {
				setError(`Unexpected error (HTTP ${response.status})`);
			}
		} catch {
			setError("Cannot connect to server");
		} finally {
			setLoading(false);
		}
	}

	return (
		<div className="flex min-h-screen items-center justify-center bg-surface-0 px-4">
			<div className="w-full max-w-sm space-y-8">
				{/* Branding */}
				<div className="flex flex-col items-center gap-3">
					<div className="flex items-center justify-center w-12 h-12 rounded-xl bg-accent/10">
						<Activity size={24} className="text-accent" />
					</div>
					<h1 className="text-xl font-bold text-text-primary">Cortex STT Server</h1>
					<p className="text-sm text-text-muted">Enter your API key to continue</p>
				</div>

				{/* Form */}
				<form onSubmit={handleSubmit} className="space-y-5">
					<Input
						label="API Key"
						type="password"
						placeholder="Enter your API key"
						value={key}
						onChange={(e) => {
							setKey(e.target.value);
							if (error) setError(null);
						}}
						autoFocus
						autoComplete="current-password"
					/>

					{error && (
						<p className="text-sm text-error text-center">{error}</p>
					)}

					<Button
						type="submit"
						size="lg"
						loading={loading}
						className="w-full"
					>
						Sign in
					</Button>
				</form>
			</div>
		</div>
	);
}
