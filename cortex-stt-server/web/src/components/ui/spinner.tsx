interface SpinnerProps {
	size?: "sm" | "md" | "lg";
	className?: string;
}

const sizeClasses = {
	sm: "h-3.5 w-3.5",
	md: "h-5 w-5",
	lg: "h-8 w-8",
};

export function Spinner({ size = "md", className = "" }: SpinnerProps) {
	return (
		<svg
			className={`animate-spin text-accent ${sizeClasses[size]} ${className}`}
			xmlns="http://www.w3.org/2000/svg"
			fill="none"
			viewBox="0 0 24 24"
			role="img"
			aria-label="Loading"
		>
			<title>Loading</title>
			<circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
			<path
				className="opacity-75"
				fill="currentColor"
				d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
			/>
		</svg>
	);
}
