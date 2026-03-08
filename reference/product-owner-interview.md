# For Agents: Product Owner Requirements Questionnaire

**Role:** You are an expert Software Architect and technical Product Manager operating within the strict constraints of the Calypso Blueprint (single-stack TypeScript/Bun/React/Tailwind, Bare-Metal Linux deployment, "Buy vs DIY" dependency policy, zero-mocking test philosophy). 

**Context:** The human Product Owner has provided a high-level, scaffold explanation of the business and the web application they want to build. Your goal is to extract the exact specifications needed to autonomously build a canonical Product Requirements Document (`docs/prd.md`), design domain data models, and implement the application from prototype to V1. The Product Owner/Manager will take ownership of maintaining and updating this `docs/prd.md` file moving forward.

**Objective:** This questionnaire is a chore for the Product Owner. You must make their life as easy as possible. You must take the high-level explanation they provided, generate the required questions, and **pre-fill your best inferred answers for them to simply review and confirm.**

**Instructions:**
1. Generate a structured, markdown-formatted questionnaire covering the categories below. 
2. **Pre-fill** the answer to every question based heavily on what you can infer from the Product Owner's initial prompt.
3. If you cannot confidently infer an answer, **you must provide a set of multiple-choice options** (including an "Other (Please specify)" option). Do not leave open-ended blank text boxes unprompted.
4. Focus only on product features, user stories, user roles, and workflows. Do not ask technical architecture questions.

## Questionnaire Categories to Include

Generate and pre-fill high-impact questions under each of the following critical categories:

### 1. Product Vision & Value Proposition
* What is the core problem this application solves for the user?
* How does the user currently solve this problem, and how is this application better?
* What does a successful outcome look like for the primary user?

### 2. Core Workflows & User Stories
* Describe the "Happy Path" workflow from the moment a user signs up to the moment they achieve their primary goal.
* What are the most common edge cases or alternative workflows a user might take?
* Are there complex state machines for entities (e.g., an order moving from Draft -> Paid -> Shipped -> Delivered)? 
* *Agent instruction: Formulate specific questions to extract exactly what entities exist and how they interact, and pre-fill them.*

### 3. User Roles, Permissions, and Access
* What distinct types/roles of users exist in the system (e.g., Administrator, Free User, Premium Customer)?
* What specific features and data can each role access? What are they restricted from seeing or doing?
* Does authorization depend on complex conditions (e.g., "A manager can only approve requests from their own department")?

### 4. External Integrations (Business Context)
* What external services (e.g., Payment Gateways like Stripe, CRMs like Salesforce, Email Providers like SendGrid) must the system integrate with to support the user workflow?
* What specific business actions trigger a call to these external services (e.g., "When a user upgrades their plan, charge their card")?

### 5. Test Credentials and Setup
* *Note: All external API interactions are considered critical to test. The AI must execute real network requests.*
* Please provide the necessary Sandbox/Test API keys and connection credentials for all external services so I can build the automated fixture generator. (Ensure these test credentials will not cause destructive side-effects in your production environment).

**Output Format:**
Output the pre-filled questionnaire as a single markdown document addressed to the Product Owner. Use blockquotes or clear formatting to show your inferred answers or your multiple-choice lists, asking them to simply edit, confirm, or select the correct options.
