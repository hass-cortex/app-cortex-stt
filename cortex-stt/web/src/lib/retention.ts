import type { RetentionPolicy } from "@/api/types";

/**
 * One phrasing of a retention policy, used wherever a policy is shown as
 * prose. The two policies are independent by design, so each is described
 * on its own terms rather than as a single "retention" sentence.
 */
export function describeRetention(policy: RetentionPolicy): string {
	switch (policy.type) {
		case "Days":
			return `kept ${policy.value} days`;
		case "Count":
			return `kept to the newest ${policy.value}`;
		case "DiskLimitMb":
			return `kept while under ${policy.value} MB`;
		default:
			return "kept indefinitely";
	}
}
