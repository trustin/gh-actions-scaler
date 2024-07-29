gh-actions-scaler automatically scales in or out your [GitHub Actions Self-hosted Runners](https://docs.github.com/en/actions/hosting-your-own-runners) running as Docker containers.

## Architecture overview

![[What is gh-actions-scaler? 2024-07-29 21.15.15.excalidraw]]
The autoscaler is responsible for:

- Checking the state of GitHub workflow service periodically or via webhooks to determine:
	- the ideal number of self-hosted runner containers; and
	- the ideal number of machines that run the self-hosted runner containers.
- Establishing an SSH (Secure SHell) session to each machine to create or destroy the self-hosted runner containers based on the job queue state.
- (Optional) (De-)provisioning the machines using the API from the cloud vendors like AWS based on the job queue state.
