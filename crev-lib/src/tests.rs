use super::*;

use crev_data::{proof::trust::TrustLevel, Digest, OwnId};
use default::default;
use std::str::FromStr;

// Basic liftime of an `LockedId`:
//
// * generate
// * lock with a passphrase
// * unlock
// * compare
#[test]
fn lock_and_unlock() -> Result<()> {
    let id = OwnId::generate_for_git_url("https://example.com/crev-proofs");

    let id_relocked = id::LockedId::from_own_id(&id, "password")?.to_unlocked("password")?;
    assert_eq!(id.id.id, id_relocked.id.id);

    assert!(id::LockedId::from_own_id(&id, "password")?
        .to_unlocked("wrongpassword")
        .is_err());

    let id_stored = serde_yaml::to_string(&id::LockedId::from_own_id(&id, "pass")?)?;
    let id_restored: OwnId =
        serde_yaml::from_str::<id::LockedId>(&id_stored)?.to_unlocked("pass")?;

    println!("{}", id_stored);

    assert_eq!(id.id.id, id_restored.id.id);
    Ok(())
}

#[test]
fn use_id_generated_by_previous_versions() -> Result<()> {
    let yaml = r#"
---
version: -1
url: "https://github.com/dpc/crev-proofs-test"
public-key: V4HcWyFSKZPSnLJWFAiGkm0nuue4USDnNAdibRvX4gQ
sealed-secret-key: Jcpm8spOQjpsQ97Wpnh0iXfWiBFYOVy4r-7G6EV4wE7tXCiemg4_m1qcTS2md0cq
seal-nonce: eub5pGojkzN57H62I4EesgYgoECJT1vcnkm2VukSZws
pass:
  version: 19
  variant: argon2id
  iterations: 192
  memory-size: 4096
  salt: EKf-mqQyKBEsPrWu2kpaiMPQDpdnPuCULNv6OVwHk1Y
"#;

    let locked = id::LockedId::from_str(yaml)?;
    let unlocked = locked.to_unlocked("a")?;

    let _trust_proof = unlocked
        .as_pubid()
        .create_trust_proof(vec![unlocked.as_pubid().to_owned()], TrustLevel::High)?
        .sign_by(&unlocked)?;

    Ok(())
}

#[test]
fn validate_proof_generated_by_previous_version() -> Result<()> {
    let yaml = r#"
-----BEGIN CREV PACKAGE REVIEW-----
version: -1
date: "2018-12-18T23:10:21.111854021-08:00"
from:
  id-type: crev
  id: 8iUv_SPgsAQ4paabLfs1D9tIptMnuSRZ344_M-6m9RE
  url: "https://github.com/dpc/crev-proofs"
package:
  source: "https://crates.io"
  name: log
  version: 0.4.6
  digest: BhDmOOjfESqs8i3z9qsQANH8A39eKklgQKuVtrwN-Tw
review:
  thoroughness: low
  understanding: medium
  rating: positive
-----BEGIN CREV PACKAGE REVIEW SIGNATURE-----
LXHRP2Spd2jzaXe5CXCTwb4mu_epLtgdfxy717RSPVyUmfVxoOICg49AfKQzhpWH5bWLvFAzVuXtJnJ0klI3Dw
-----END CREV PACKAGE REVIEW-----
"#;

    let proofs = crev_data::proof::Proof::parse(yaml.as_bytes())?;
    assert_eq!(proofs.len(), 1);

    proofs[0].verify()?;

    Ok(())
}
// Exact distance of flooding the web of trust graph is configurable,
// with the edges distance corresponding to the trust level.
#[test]
fn proofdb_distance() -> Result<()> {
    let a = OwnId::generate_for_git_url("https://a");
    let b = OwnId::generate_for_git_url("https://b");
    let c = OwnId::generate_for_git_url("https://c");
    let d = OwnId::generate_for_git_url("https://d");
    let e = OwnId::generate_for_git_url("https://e");

    let distance_params = TrustDistanceParams {
        high_trust_distance: 1,
        medium_trust_distance: 10,
        low_trust_distance: 100,
        max_distance: 111,
    };

    let a_to_b = a
        .as_pubid()
        .create_trust_proof(vec![b.as_pubid().to_owned()], TrustLevel::High)?
        .sign_by(&a)?;
    let b_to_c = b
        .as_pubid()
        .create_trust_proof(vec![c.as_pubid().to_owned()], TrustLevel::Medium)?
        .sign_by(&b)?;
    let c_to_d = c
        .as_pubid()
        .create_trust_proof(vec![d.as_pubid().to_owned()], TrustLevel::Low)?
        .sign_by(&c)?;
    let d_to_e = d
        .as_pubid()
        .create_trust_proof(vec![e.as_pubid().to_owned()], TrustLevel::High)?
        .sign_by(&d)?;

    let mut trustdb = ProofDB::new();

    trustdb.import_from_iter(vec![a_to_b, b_to_c, c_to_d, d_to_e].into_iter());

    let trust_set: HashSet<crev_data::Id> = trustdb
        .calculate_trust_set(a.as_ref(), &distance_params)
        .trusted_ids()
        .cloned()
        .collect();

    assert!(trust_set.contains(a.as_ref()));
    assert!(trust_set.contains(b.as_ref()));
    assert!(trust_set.contains(c.as_ref()));
    assert!(trust_set.contains(d.as_ref()));
    assert!(!trust_set.contains(e.as_ref()));

    let b_to_d = b
        .as_pubid()
        .create_trust_proof(vec![d.as_pubid().to_owned()], TrustLevel::Medium)?
        .sign_by(&b)?;

    trustdb.import_from_iter(vec![b_to_d].into_iter());

    let trust_set: HashSet<_> = trustdb
        .calculate_trust_set(a.as_ref(), &distance_params)
        .trusted_ids()
        .cloned()
        .collect();

    assert!(trust_set.contains(a.as_ref()));
    assert!(trust_set.contains(b.as_ref()));
    assert!(trust_set.contains(c.as_ref()));
    assert!(trust_set.contains(d.as_ref()));
    assert!(trust_set.contains(e.as_ref()));
    Ok(())
}

// A subsequent review of exactly same package version
// is supposed to overwrite the previous one, and it
// should be visible in all the user-facing stats, listings
// and counts.
#[test]
fn overwritting_reviews() -> Result<()> {
    let a = OwnId::generate_for_git_url("https://a");
    let digest = vec![0; 32];
    let package = crev_data::proof::PackageInfo {
        id: None,
        source: "source".into(),
        name: "name".into(),
        version: "version".into(),
        digest: digest.clone(),
        digest_type: crev_data::proof::default_digest_type(),
        revision: "".into(),
        revision_type: crev_data::proof::default_revision_type(),
    };

    let proof1 = a
        .as_pubid()
        .create_package_review_proof(package.clone(), default(), "a".into())?
        .sign_by(&a)?;
    // it's lame, but oh well... ; we need to make sure there's a time delay between
    // the two proofs
    #[allow(deprecated)]
    std::thread::sleep_ms(1);
    let proof2 = a
        .as_pubid()
        .create_package_review_proof(package.clone(), default(), "b".into())?
        .sign_by(&a)?;

    for order in vec![
        vec![proof1.clone(), proof2.clone()],
        vec![proof2.clone(), proof1.clone()],
    ] {
        let mut trustdb = ProofDB::new();
        trustdb.import_from_iter(order.into_iter());
        assert_eq!(
            trustdb
                .get_package_reviews_by_digest(&Digest::from_vec(digest.clone()))
                .map(|r| r.comment)
                .collect::<Vec<_>>(),
            vec!["b".to_string()]
        );
        assert_eq!(
            trustdb
                .get_package_reviews_for_package(
                    &package.source,
                    Some(&package.name),
                    Some(&package.version)
                )
                .count(),
            1
        );
        assert_eq!(
            trustdb
                .get_package_reviews_for_package(&package.source, Some(&package.name), None)
                .count(),
            1
        );
        assert_eq!(
            trustdb
                .get_package_reviews_for_package(&package.source, None, None)
                .count(),
            1
        );
    }

    Ok(())
}

#[test]
fn proofdb_distrust() -> Result<()> {
    let a = OwnId::generate_for_git_url("https://a");
    let b = OwnId::generate_for_git_url("https://b");
    let c = OwnId::generate_for_git_url("https://c");
    let d = OwnId::generate_for_git_url("https://d");
    let e = OwnId::generate_for_git_url("https://e");

    let distance_params = TrustDistanceParams {
        high_trust_distance: 1,
        medium_trust_distance: 10,
        low_trust_distance: 100,
        max_distance: 10000,
    };

    let a_to_bc = a
        .as_pubid()
        .create_trust_proof(
            vec![b.as_pubid().to_owned(), c.as_pubid().to_owned()],
            TrustLevel::High,
        )?
        .sign_by(&a)?;
    let b_to_d = b
        .as_pubid()
        .create_trust_proof(vec![d.as_pubid().to_owned()], TrustLevel::Low)?
        .sign_by(&b)?;
    let d_to_c = d
        .as_pubid()
        .create_trust_proof(vec![c.as_pubid().to_owned()], TrustLevel::Distrust)?
        .sign_by(&d)?;
    let c_to_e = c
        .as_pubid()
        .create_trust_proof(vec![e.as_pubid().to_owned()], TrustLevel::High)?
        .sign_by(&c)?;

    let mut trustdb = ProofDB::new();

    trustdb.import_from_iter(vec![a_to_bc, b_to_d, d_to_c, c_to_e].into_iter());

    let trust_set: HashSet<_> = trustdb
        .calculate_trust_set(a.as_ref(), &distance_params)
        .trusted_ids()
        .cloned()
        .collect();

    assert!(trust_set.contains(a.as_ref()));
    assert!(trust_set.contains(b.as_ref()));
    assert!(!trust_set.contains(c.as_ref()));
    assert!(trust_set.contains(d.as_ref()));
    assert!(!trust_set.contains(e.as_ref()));

    let e_to_d = e
        .as_pubid()
        .create_trust_proof(vec![d.as_pubid().to_owned()], TrustLevel::Distrust)?
        .sign_by(&e)?;

    trustdb.import_from_iter(vec![e_to_d].into_iter());

    let trust_set: HashSet<_> = trustdb
        .calculate_trust_set(a.as_ref(), &distance_params)
        .trusted_ids()
        .cloned()
        .collect();

    assert!(trust_set.contains(a.as_ref()));
    assert!(trust_set.contains(b.as_ref()));
    assert!(!trust_set.contains(c.as_ref()));
    assert!(!trust_set.contains(d.as_ref()));
    assert!(!trust_set.contains(e.as_ref()));

    Ok(())
}
