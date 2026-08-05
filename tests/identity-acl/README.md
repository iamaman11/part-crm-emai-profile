# Identity and ACL negative fixtures

`fixtures/assignment-authorizes.sql` deliberately projects an active historical
profile/client assignment as authorization. The permanent Quality Gate executes
the Step 4 ACL suite with this fixture and requires the suite to fail, proving
that assignments and explicit grants remain separate concepts.

The positive path is covered by the same suite without the fixture: an active
tenant owner is authorized by role, while an active member must have an explicit
client or profile grant. Missing, foreign, suspended, revoked and insufficiently
granted cases use the same disclosure-neutral result shape.
