use proptest::prelude::*;

use super::*;

fn ada() -> OwnerName {
    "Ada Lovelace".parse().expect("valid owner name")
}

fn open_with(cents: u64) -> State {
    State::Open {
        owner: ada(),
        balance: Cents::new(cents),
    }
}

fn deposit_of(cents: u64) -> Command {
    Command::Deposit {
        amount: Cents::new(cents),
    }
}

fn withdrawal_of(cents: u64) -> Command {
    Command::Withdraw {
        amount: Cents::new(cents),
    }
}

#[test]
fn opening_a_new_account_records_the_owner() {
    let events = decide(&Command::Open { owner: ada() }, &State::NotOpened);

    assert_eq!(events, Ok(vec![Event::Opened { owner: ada() }]));
}

#[test]
fn opened_account_starts_with_zero_balance() {
    let state = fold(&[Event::Opened { owner: ada() }]);

    assert_eq!(state, open_with(0));
}

#[test]
fn rejects_opening_an_open_account() {
    let result = decide(&Command::Open { owner: ada() }, &open_with(0));

    assert_eq!(result, Err(Rejection::AlreadyOpen));
}

#[test]
fn rejects_deposit_before_opening() {
    assert_eq!(
        decide(&deposit_of(100), &State::NotOpened),
        Err(Rejection::NotOpened)
    );
}

#[test]
fn rejects_zero_deposit() {
    assert_eq!(
        decide(&deposit_of(0), &open_with(0)),
        Err(Rejection::ZeroAmount)
    );
}

#[test]
fn rejects_zero_withdrawal() {
    assert_eq!(
        decide(&withdrawal_of(0), &open_with(10)),
        Err(Rejection::ZeroAmount)
    );
}

#[test]
fn rejects_deposit_that_would_overflow_the_balance() {
    let result = decide(&deposit_of(2), &open_with(u64::MAX - 1));

    assert_eq!(
        result,
        Err(Rejection::BalanceOverflow {
            balance: Cents::new(u64::MAX - 1),
            deposit: Cents::new(2),
        })
    );
}

#[test]
fn withdrawal_within_balance_reduces_it() {
    let state = open_with(500);

    let events = decide(&withdrawal_of(200), &state).expect("funds suffice");

    assert_eq!(fold_onto(state, &events), open_with(300));
}

#[test]
fn rejects_withdrawal_over_balance() {
    let result = decide(&withdrawal_of(501), &open_with(500));

    assert_eq!(
        result,
        Err(Rejection::InsufficientFunds {
            balance: Cents::new(500),
            requested: Cents::new(501),
        })
    );
}

#[test]
fn closing_an_empty_account_records_it_closed() {
    assert_eq!(
        decide(&Command::Close, &open_with(0)),
        Ok(vec![Event::Closed])
    );
}

#[test]
fn rejects_closing_an_account_holding_money() {
    assert_eq!(
        decide(&Command::Close, &open_with(1)),
        Err(Rejection::BalanceNotZero {
            balance: Cents::new(1)
        })
    );
}

#[test]
fn closed_account_rejects_every_command() {
    let closed = fold(&[Event::Opened { owner: ada() }, Event::Closed]);
    let commands = [
        Command::Open { owner: ada() },
        deposit_of(1),
        withdrawal_of(1),
        Command::Close,
    ];

    let results = commands.map(|command| decide(&command, &closed));

    assert_eq!(results, [const { Err(Rejection::AccountClosed) }; 4]);
}

#[test]
fn evolve_ignores_events_decide_cannot_produce() {
    let state = evolve(
        State::NotOpened,
        &Event::Deposited {
            amount: Cents::new(5),
        },
    );

    assert_eq!(state, State::NotOpened);
}

fn fold_onto(state: State, events: &[Event]) -> State {
    events.iter().fold(state, evolve)
}

/// Applies `commands` in order to an account opened for Ada, keeping only the
/// events of accepted commands. Rejections are expected and skipped.
fn run(commands: &[Command]) -> (Vec<Event>, State) {
    let mut history = vec![Event::Opened { owner: ada() }];
    let mut state = fold(&history);
    for command in commands {
        if let Ok(events) = decide(command, &state) {
            state = fold_onto(state, &events);
            history.extend(events);
        }
    }
    (history, state)
}

fn money_command() -> impl Strategy<Value = Command> {
    prop_oneof![
        (0..10_000_u64).prop_map(deposit_of),
        (0..10_000_u64).prop_map(withdrawal_of),
    ]
}

proptest! {
    #[test]
    fn balance_never_goes_below_zero(commands in prop::collection::vec(money_command(), 0..64)) {
        let (history, state) = run(&commands);

        // Net flow over accepted events, computed in a type that can go negative.
        let mut net: i128 = 0;
        for event in &history {
            match event {
                Event::Deposited { amount } => net += i128::from(amount.get()),
                Event::Withdrew { amount } => net -= i128::from(amount.get()),
                Event::Opened { .. } | Event::Closed => {}
            }
            prop_assert!(net >= 0, "net flow went negative: {net}");
        }
        prop_assert_eq!(state, State::Open { owner: ada(), balance: Cents::new(u64::try_from(net)?) });
    }

    #[test]
    fn replaying_decided_events_reproduces_state(
        commands in prop::collection::vec(money_command(), 0..64),
    ) {
        let (history, state) = run(&commands);

        prop_assert_eq!(fold(&history), state);
    }

    #[test]
    fn withdrawing_a_deposit_restores_the_state(
        start in 0..1_000_000_u64,
        cents in 1..1_000_000_u64,
    ) {
        let before = open_with(start);

        let deposited = fold_onto(before.clone(), &decide(&deposit_of(cents), &before)?);
        let after = fold_onto(deposited.clone(), &decide(&withdrawal_of(cents), &deposited)?);

        prop_assert_eq!(after, before);
    }
}
