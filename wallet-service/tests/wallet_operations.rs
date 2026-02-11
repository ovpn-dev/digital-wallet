/// Integration tests for wallet operations
/// 
/// These tests require:
/// - PostgreSQL running (use docker-compose up postgres)
/// - Test database configured
/// 
/// Run with: cargo test --test wallet_operations -- --test-threads=1
/// 
/// Key concepts demonstrated:
/// - Setting up test database
/// - Testing concurrent operations
/// - Verifying optimistic locking
/// - Testing business logic errors

use rust_decimal_macros::dec;
use sqlx::PgPool;
use std::sync::Arc;
use tokio;
use wallet_service::{
    errors::WalletError,
    models::{TransactionType, TransactionStatus},
    repository::WalletRepository,
};

/// Setup test database connection
/// 
/// In real tests, you'd want:
/// - Unique database per test
/// - Transaction rollback after each test
/// - Or use testcontainers-rs
async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/wallet_test".to_string());

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}

/// Clean up test data
async fn cleanup_test_data(pool: &PgPool) {
    sqlx::query("TRUNCATE TABLE wallet_transactions, wallets CASCADE")
        .execute(pool)
        .await
        .expect("Failed to clean up test data");
}

#[tokio::test]
async fn test_create_wallet() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    // Create wallet
    let wallet = repo
        .create_wallet("test_user_1")
        .await
        .expect("Failed to create wallet");

    // Verify
    assert_eq!(wallet.user_id, "test_user_1");
    assert_eq!(wallet.balance, dec!(0));
    assert_eq!(wallet.version, 0);

    cleanup_test_data(&pool).await;
}

#[tokio::test]
async fn test_fund_wallet() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    // Create and fund wallet
    let wallet = repo.create_wallet("test_user_2").await.unwrap();
    let (updated_wallet, txn) = repo
        .fund_wallet(&wallet.id, dec!(100.50))
        .await
        .expect("Failed to fund wallet");

    // Verify wallet state
    assert_eq!(updated_wallet.balance, dec!(100.50));
    assert_eq!(updated_wallet.version, 1); // Version incremented

    // Verify transaction record
    assert_eq!(txn.wallet_id, wallet.id);
    assert_eq!(txn.amount, dec!(100.50));
    assert!(matches!(txn.transaction_type, TransactionType::Fund));
    assert!(matches!(txn.status, TransactionStatus::Completed));

    cleanup_test_data(&pool).await;
}

#[tokio::test]
async fn test_fund_wallet_with_negative_amount() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    let wallet = repo.create_wallet("test_user_3").await.unwrap();
    
    // Try to fund with negative amount
    let result = repo.fund_wallet(&wallet.id, dec!(-50)).await;

    // Should fail
    assert!(result.is_err());
    match result.unwrap_err() {
        WalletError::InvalidAmount(_) => {} // Expected
        e => panic!("Expected InvalidAmount error, got {:?}", e),
    }

    cleanup_test_data(&pool).await;
}

#[tokio::test]
async fn test_concurrent_funding() {
    let pool = setup_test_db().await;
    let repo = Arc::new(WalletRepository::new(pool.clone()));

    // Create wallet
    let wallet = repo.create_wallet("test_user_4").await.unwrap();
    let wallet_id = wallet.id.clone();

    // Launch 10 concurrent funding operations
    let mut handles = vec![];
    for _ in 0..10 {
        let repo_clone = Arc::clone(&repo);
        let wallet_id_clone = wallet_id.clone();
        
        let handle = tokio::spawn(async move {
            repo_clone
                .fund_wallet(&wallet_id_clone, dec!(10))
                .await
        });
        
        handles.push(handle);
    }

    // Wait for all to complete
    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .collect();

    // Count successes (some might fail with OptimisticLockError and should retry)
    let successes = results
        .iter()
        .filter(|r| r.as_ref().unwrap().is_ok())
        .count();

    println!("Successful operations: {}/10", successes);

    // Check final balance
    let final_wallet = repo.find_by_id(&wallet_id).await.unwrap();
    
    // Balance should match number of successful operations
    assert_eq!(
        final_wallet.balance,
        dec!(10) * rust_decimal::Decimal::from(successes)
    );

    cleanup_test_data(&pool).await;
}

#[tokio::test]
async fn test_transfer_between_wallets() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    // Create two wallets
    let wallet_a = repo.create_wallet("alice").await.unwrap();
    let wallet_b = repo.create_wallet("bob").await.unwrap();

    // Fund Alice's wallet
    repo.fund_wallet(&wallet_a.id, dec!(100)).await.unwrap();

    // Transfer from Alice to Bob
    let (out_txn, in_txn) = repo
        .transfer(&wallet_a.id, &wallet_b.id, dec!(30))
        .await
        .expect("Transfer failed");

    // Verify transactions
    assert_eq!(out_txn.amount, dec!(30));
    assert!(matches!(out_txn.transaction_type, TransactionType::TransferOut));
    assert_eq!(in_txn.amount, dec!(30));
    assert!(matches!(in_txn.transaction_type, TransactionType::TransferIn));
    assert_eq!(out_txn.reference_id, in_txn.reference_id); // Linked

    // Verify final balances
    let alice_final = repo.find_by_id(&wallet_a.id).await.unwrap();
    let bob_final = repo.find_by_id(&wallet_b.id).await.unwrap();

    assert_eq!(alice_final.balance, dec!(70));
    assert_eq!(bob_final.balance, dec!(30));

    cleanup_test_data(&pool).await;
}

#[tokio::test]
async fn test_transfer_insufficient_balance() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    // Create two wallets
    let wallet_a = repo.create_wallet("alice").await.unwrap();
    let wallet_b = repo.create_wallet("bob").await.unwrap();

    // Fund Alice with only $10
    repo.fund_wallet(&wallet_a.id, dec!(10)).await.unwrap();

    // Try to transfer $50 (more than balance)
    let result = repo.transfer(&wallet_a.id, &wallet_b.id, dec!(50)).await;

    // Should fail
    assert!(result.is_err());
    match result.unwrap_err() {
        WalletError::InsufficientBalance { required, available } => {
            assert_eq!(required, dec!(50));
            assert_eq!(available, dec!(10));
        }
        e => panic!("Expected InsufficientBalance error, got {:?}", e),
    }

    cleanup_test_data(&pool).await;
}

#[tokio::test]
async fn test_transfer_to_same_wallet() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    let wallet = repo.create_wallet("alice").await.unwrap();
    repo.fund_wallet(&wallet.id, dec!(100)).await.unwrap();

    // Try to transfer to same wallet
    let result = repo.transfer(&wallet.id, &wallet.id, dec!(50)).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WalletError::InvalidAmount(_) => {} // Expected
        e => panic!("Expected InvalidAmount error, got {:?}", e),
    }

    cleanup_test_data(&pool).await;
}

#[tokio::test]
async fn test_find_user_wallets() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    // Create multiple wallets for same user
    let wallet1 = repo.create_wallet("alice").await.unwrap();
    let wallet2 = repo.create_wallet("alice").await.unwrap();
    let _wallet3 = repo.create_wallet("bob").await.unwrap(); // Different user

    // Find Alice's wallets
    let alice_wallets = repo
        .find_by_user_id("alice")
        .await
        .expect("Failed to find wallets");

    assert_eq!(alice_wallets.len(), 2);
    assert!(alice_wallets.iter().any(|w| w.id == wallet1.id));
    assert!(alice_wallets.iter().any(|w| w.id == wallet2.id));

    cleanup_test_data(&pool).await;
}

/// Example of testing data consistency
#[tokio::test]
async fn test_data_consistency_after_multiple_operations() {
    let pool = setup_test_db().await;
    let repo = WalletRepository::new(pool.clone());

    // Create wallet
    let wallet = repo.create_wallet("test_user").await.unwrap();

    // Perform multiple operations
    repo.fund_wallet(&wallet.id, dec!(100)).await.unwrap();
    repo.fund_wallet(&wallet.id, dec!(50)).await.unwrap();
    
    // Check balance matches sum of transactions
    let final_wallet = repo.find_by_id(&wallet.id).await.unwrap();
    assert_eq!(final_wallet.balance, dec!(150));

    // Verify in database directly
    let db_balance: (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT balance FROM wallets WHERE id = $1"
    )
    .bind(&wallet.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(db_balance.0, dec!(150));

    cleanup_test_data(&pool).await;
}
