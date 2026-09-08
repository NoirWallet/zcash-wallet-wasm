use super::*;
use rusqlite::OptionalExtension;

#[tokio::test]
async fn helper_recovery_pending_prior_proposal_does_not_block_later_cast() {
    let fixture = BallotFixture::new(true).await;
    let session = &fixture.session;
    for proposal in fixture.roster() {
        session
            .db
            .set_ballot_intent(ID, proposal, zcash_voting::session::Decision::Choice(0), 2)
            .unwrap();
    }
    let recoveries = vote::synthetic_recovery_fixtures(
        &session.db,
        ID,
        0,
        &[DraftVote {
            proposal_id: 1,
            choice: 0,
            num_options: 2,
            single_share: false,
            vc_tree_position: 0,
        }],
    )
    .unwrap();
    let canonical = BallotFixture::store(&session.db, &recoveries);
    session
        .db
        .prepare_vote_submission(ID, 0, &canonical)
        .unwrap();
    session.db.mark_vote_dispatching(ID, 0, 1).unwrap();
    let events: Vec<zcash_voting::confirmation::TxEvent> =
        serde_json::from_value(serde_json::json!([{
            "type": "cast_vote", "attributes": [
                {"key":"vote_round_id", "value":ID}, {"key":"leaf_index", "value":"50,51"}
            ]
        }]))
        .unwrap();
    session
        .db
        .confirm_journaled_vote(ID, 0, 1, canonical.tx_hash(), &events)
        .unwrap();
    let durable = Arc::new(Mutex::new(session.db.serialize_bytes().unwrap()));
    assert!(plan_snapshot(&durable.lock().unwrap()).is_none());
    let transport = fixture.transport(durable.clone(), Stop::None);
    let builds = Cell::new(0);
    let error = session
        .submit_ballot_with(
            ID,
            None,
            &transport,
            || std::future::ready(Ok(fixture.round.clone())),
            |_, bundle, drafts| {
                assert_eq!(bundle, 0);
                assert_eq!(
                    drafts
                        .iter()
                        .map(|draft| draft.proposal_id)
                        .collect::<Vec<_>>(),
                    [2]
                );
                builds.set(builds.get() + 1);
                std::future::ready(Err(VotingApiError::new(VotingErrorCode::ProofFailed)))
            },
            || async {
                *durable.lock().unwrap() = session.db.serialize_bytes().unwrap();
                Ok(())
            },
            || Ok(1500),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, VotingErrorCode::ProofFailed);
    assert_eq!(builds.get(), 1);
    assert_eq!(session.db.get_share_delegations(ID).unwrap().len(), 16);
}

fn plan_snapshot(bytes: &[u8]) -> Option<String> {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.deserialize_read_exact(
        rusqlite::MAIN_DB,
        std::io::Cursor::new(bytes),
        bytes.len(),
        true,
    )
    .unwrap();
    conn.query_row(
        "SELECT share_plans_json FROM helper_share_plans ORDER BY proposal_id LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .unwrap()
}

struct Helpers {
    vote: RecordingTransport,
    posts: Mutex<Vec<(String, Vec<u8>)>>,
    plan: Arc<Mutex<Option<String>>>,
    payloads: Arc<Mutex<std::collections::BTreeMap<u32, Vec<u8>>>>,
    stop_post: Option<usize>,
    pending_until_post: std::sync::atomic::AtomicBool,
    one_confirmation: bool,
}

impl Helpers {
    fn new(fixture: &BallotFixture, durable: Arc<Mutex<Vec<u8>>>) -> Self {
        let mut vote = fixture.transport(durable, Stop::None);
        vote.confirm_helpers = true;
        Self {
            vote,
            posts: Mutex::new(vec![]),
            plan: Default::default(),
            payloads: Default::default(),
            stop_post: None,
            pending_until_post: Default::default(),
            one_confirmation: false,
        }
    }
}

impl HttpTransport for Helpers {
    fn execute(&self, request: OwnedHttpRequest) -> TransportFuture {
        if request.url.contains("/share-status/")
            && (self
                .pending_until_post
                .load(std::sync::atomic::Ordering::SeqCst)
                || (self.one_confirmation && request.url.starts_with("https://helper-2.example/")))
        {
            return Box::pin(std::future::ready(Ok(OwnedHttpResponse {
                status: 200,
                body: br#"{"status":"pending"}"#.to_vec(),
                content_type: Some("application/json".into()),
            })));
        }
        if !request.url.ends_with("/shares") {
            return self.vote.execute(request);
        }
        assert_eq!(request.method, "POST");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "Content-Type" && value == "application/json")
        );
        let bytes = self.vote.durable.lock().unwrap().clone();
        let plan = plan_snapshot(&bytes)
            .expect("complete plan must be checkpointed before every POST construction");
        let mut expected_plan = self.plan.lock().unwrap();
        if let Some(expected) = &*expected_plan {
            assert!(expected == &plan, "immutable helper plan changed");
        } else {
            *expected_plan = Some(plan);
        }
        drop(expected_plan);
        let db = VotingDb::open_browser(
            &self.vote.wallet_id,
            zcash_voting::Network::Testnet,
            Some(&bytes),
        )
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let index = wire["share_index"].as_u64().unwrap() as u32;
        let base = request
            .url
            .strip_suffix("/shielded-vote/v1/shares")
            .unwrap();
        let shares = db.get_share_delegations(ID).unwrap();
        let share = shares
            .iter()
            .find(|share| share.share_index == index)
            .unwrap();
        assert!(
            share
                .attempting_urls
                .iter()
                .chain(&share.ambiguous_urls)
                .chain(&share.sent_to_urls)
                .any(|url| url == base),
            "attempt must be checkpointed before every POST"
        );
        let mut payloads = self.payloads.lock().unwrap();
        if let Some(expected) = payloads.get(&index) {
            assert!(
                expected == &request.body,
                "recovery regenerated a share or its schedule"
            );
        } else {
            payloads.insert(index, request.body.clone());
        }
        drop(payloads);
        let mut posts = self.posts.lock().unwrap();
        posts.push((request.url.clone(), request.body.clone()));
        let interrupted = self.stop_post == Some(posts.len());
        drop(posts);
        self.pending_until_post
            .store(false, std::sync::atomic::Ordering::SeqCst);
        if interrupted {
            Box::pin(pending())
        } else {
            self.vote.execute(request)
        }
    }
}

async fn confirmed_vote_base() -> (BallotFixture, Vec<u8>) {
    let fixture = BallotFixture::new(false).await;
    let session = &fixture.session;
    let durable = Arc::new(Mutex::new(session.db.serialize_bytes().unwrap()));
    let transport = fixture.transport(durable.clone(), Stop::None);
    let checkpoints = Cell::new(0);
    let mut future = Box::pin(session.submit_ballot_with(
        ID,
        Some(vec![BallotDecision::Choice {
            proposal_id: 1,
            option: 0,
        }]),
        &transport,
        || std::future::ready(Ok(fixture.round.clone())),
        |_, _, _| std::future::ready(Ok(BallotFixture::store(&session.db, &fixture.recoveries))),
        || async {
            *durable.lock().unwrap() = session.db.serialize_bytes().unwrap();
            checkpoints.set(checkpoints.get() + 1);
            if checkpoints.get() == 5 {
                pending::<()>().await;
            }
            Ok(())
        },
        || Ok(1500),
    ));
    assert!(poll!(&mut future).is_pending());
    drop(future);
    let bytes = durable.lock().unwrap().clone();
    assert!(plan_snapshot(&bytes).is_none());
    (fixture, bytes)
}

#[tokio::test]
async fn helper_recovery_reopens_every_plan_attempt_outcome_and_confirmation_checkpoint() {
    let started = std::time::Instant::now();
    let (fixture, base) = confirmed_vote_base().await;
    eprintln!(
        "helper checkpoint fixture ready after {:?}",
        started.elapsed()
    );
    // One complete-plan checkpoint, two per initial POST, one per quorum.
    for boundary in 1..=49 {
        for saved in [false, true] {
            let case_started = std::time::Instant::now();
            eprintln!(
                "helper checkpoint {boundary}/49 saved={saved} start elapsed={:?}",
                started.elapsed()
            );
            let session = fixture.reopen(&base);
            let durable = Arc::new(Mutex::new(base.clone()));
            let transport = Helpers::new(&fixture, durable.clone());
            let checkpoints = Cell::new(0);
            let mut future = Box::pin(session.submit_ballot_with(
                ID,
                None,
                &transport,
                || std::future::ready(Ok(fixture.round.clone())),
                |_, _, _| async { panic!("helper resume regenerated a vote") },
                || async {
                    checkpoints.set(checkpoints.get() + 1);
                    if checkpoints.get() == boundary && !saved {
                        pending::<()>().await;
                    }
                    *durable.lock().unwrap() = session.db.serialize_bytes().unwrap();
                    if checkpoints.get() == boundary && saved {
                        pending::<()>().await;
                    }
                    Ok(())
                },
                || Ok(1500),
            ));
            assert!(
                poll!(&mut future).is_pending(),
                "checkpoint {boundary}, saved {saved}"
            );
            drop(future);
            let bytes = durable.lock().unwrap().clone();
            let saved_plan = plan_snapshot(&bytes);
            if boundary <= 2 {
                assert_eq!(transport.posts.lock().unwrap().len(), 0);
            }
            let reopened = fixture.reopen(&bytes);
            let result = reopened
                .submit_ballot_with(
                    ID,
                    None,
                    &transport,
                    || std::future::ready(Ok(fixture.round.clone())),
                    |_, _, _| async { panic!("helper resume regenerated a vote") },
                    || async {
                        *durable.lock().unwrap() = reopened.db.serialize_bytes().unwrap();
                        Ok(())
                    },
                    || Ok(1500),
                )
                .await
                .unwrap();
            assert_eq!(
                result.phase,
                VotingRoundPhase::SharesConfirmed,
                "checkpoint {boundary}, saved {saved}"
            );
            let shares = reopened.db.get_share_delegations(ID).unwrap();
            assert_eq!(shares.len(), 16);
            assert!(
                shares
                    .iter()
                    .all(|share| share.confirmed && share.target_count == 1)
            );
            if let Some(saved_plan) = saved_plan {
                assert_eq!(
                    plan_snapshot(&durable.lock().unwrap()).as_ref(),
                    Some(&saved_plan)
                );
            }
            assert_eq!(
                reopened
                    .round_status_at(ID, 1500)
                    .unwrap()
                    .confirmed_helpers,
                16
            );
            fixture.assert_canonical(&reopened);
            eprintln!(
                "helper checkpoint {boundary}/49 saved={saved} passed in {:?}",
                case_started.elapsed()
            );
        }
    }
}

#[tokio::test]
async fn helper_recovery_interruption_during_each_eager_post_preserves_payloads_and_plan() {
    let (fixture, base) = confirmed_vote_base().await;
    for index in 1..=16 {
        let session = fixture.reopen(&base);
        let durable = Arc::new(Mutex::new(base.clone()));
        let mut transport = Helpers::new(&fixture, durable.clone());
        transport.stop_post = Some(index);
        let mut future = Box::pin(session.submit_ballot_with(
            ID,
            None,
            &transport,
            || std::future::ready(Ok(fixture.round.clone())),
            |_, _, _| async { panic!("helper resume regenerated a vote") },
            || async {
                *durable.lock().unwrap() = session.db.serialize_bytes().unwrap();
                Ok(())
            },
            || Ok(1500),
        ));
        assert!(poll!(&mut future).is_pending());
        drop(future);
        let bytes = durable.lock().unwrap().clone();
        let reopened = fixture.reopen(&bytes);
        assert!(
            reopened
                .db
                .get_share_delegations(ID)
                .unwrap()
                .iter()
                .any(|share| !share.attempting_urls.is_empty())
        );
        transport.stop_post = None;
        let before_resume = transport.posts.lock().unwrap().len();
        if index == 1 {
            transport
                .pending_until_post
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let result = reopened
            .submit_ballot_with(
                ID,
                None,
                &transport,
                || std::future::ready(Ok(fixture.round.clone())),
                |_, _, _| async { panic!("helper resume regenerated a vote") },
                || async {
                    *durable.lock().unwrap() = reopened.db.serialize_bytes().unwrap();
                    Ok(())
                },
                || Ok(1500),
            )
            .await
            .unwrap();
        assert_eq!(result.phase, VotingRoundPhase::SharesConfirmed);
        if index == 1 {
            assert!(
                transport.posts.lock().unwrap().len() > before_resume,
                "an interrupted reservation must use authorized recovery when quorum is not yet observed"
            );
        }
    }
}

#[tokio::test]
async fn helper_recovery_requires_two_configured_confirmations_not_placement_target() {
    let (fixture, base) = confirmed_vote_base().await;
    let session = fixture.reopen(&base);
    let durable = Arc::new(Mutex::new(base));
    let mut transport = Helpers::new(&fixture, durable.clone());
    transport.one_confirmation = true;
    let result = session
        .submit_ballot_with(
            ID,
            None,
            &transport,
            || std::future::ready(Ok(fixture.round.clone())),
            |_, _, _| async { panic!("helper resume regenerated a vote") },
            || async {
                *durable.lock().unwrap() = session.db.serialize_bytes().unwrap();
                Ok(())
            },
            || Ok(1500),
        )
        .await
        .unwrap();
    assert_eq!(result.phase, VotingRoundPhase::VoteConfirmed);
    assert_eq!(
        session.round_status_at(ID, 1500).unwrap().confirmed_helpers,
        0
    );
    assert!(
        session
            .db
            .get_share_delegations(ID)
            .unwrap()
            .iter()
            .all(|share| share.target_count == 1 && !share.confirmed)
    );
    let reopened = fixture.reopen(&durable.lock().unwrap());
    transport.one_confirmation = false;
    let result = reopened
        .submit_ballot_with(
            ID,
            None,
            &transport,
            || std::future::ready(Ok(fixture.round.clone())),
            |_, _, _| async { panic!("helper resume regenerated a vote") },
            || async {
                *durable.lock().unwrap() = reopened.db.serialize_bytes().unwrap();
                Ok(())
            },
            || Ok(1500),
        )
        .await
        .unwrap();
    assert_eq!(result.phase, VotingRoundPhase::SharesConfirmed);
}

#[tokio::test]
async fn helper_recovery_checkpoint_failure_and_expiry_never_dispatch_a_post() {
    let (fixture, base) = confirmed_vote_base().await;
    for boundary in [1, 2] {
        for expiry in [false, true] {
            let session = fixture.reopen(&base);
            let durable = Arc::new(Mutex::new(base.clone()));
            let transport = Helpers::new(&fixture, durable.clone());
            let checkpoints = Cell::new(0);
            let now = Cell::new(1500);
            let result = session
                .submit_ballot_with(
                    ID,
                    None,
                    &transport,
                    || std::future::ready(Ok(fixture.round.clone())),
                    |_, _, _| async { panic!("helper resume regenerated a vote") },
                    || async {
                        checkpoints.set(checkpoints.get() + 1);
                        if checkpoints.get() == boundary {
                            if expiry {
                                now.set(2000);
                            } else {
                                return Err(VotingApiError::new(
                                    VotingErrorCode::PersistenceFailed,
                                ));
                            }
                        }
                        *durable.lock().unwrap() = session.db.serialize_bytes().unwrap();
                        Ok(())
                    },
                    || Ok(now.get()),
                )
                .await;
            assert_eq!(transport.posts.lock().unwrap().len(), 0);
            if expiry {
                assert!(
                    result.is_ok() || result.unwrap_err().code == VotingErrorCode::RoundExpired
                );
            } else {
                assert_eq!(result.unwrap_err().code, VotingErrorCode::PersistenceFailed);
            }
        }
    }
}
