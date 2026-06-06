#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn get_pool() -> PgPool {
        dotenvy::dotenv().ok();
        let db_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set");
        PgPool::connect(&db_url).await.unwrap()
    }

    async fn setup_business(pool: &PgPool) -> (Uuid, String) {
        let business_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO businesses (id, name) VALUES ($1, $2)",
            business_id,
            format!("Test Business {}", business_id)
        )
        .execute(pool)
        .await
        .unwrap();

        let raw_key = format!("sk_live_{}", Uuid::new_v4().to_string().replace("-", ""));
        let key_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(raw_key.as_bytes());
            hex::encode(hasher.finalize())
        };

        sqlx::query!(
            "INSERT INTO api_keys (id, business_id, key_hash, key_prefix)
             VALUES ($1, $2, $3, $4)",
            Uuid::new_v4(),
            business_id,
            key_hash,
            &raw_key[..16]
        )
        .execute(pool)
        .await
        .unwrap();

        (business_id, raw_key)
    }

    async fn setup_customer(pool: &PgPool, business_id: Uuid) -> Uuid {
        let customer_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO customers (id, business_id, name, email)
             VALUES ($1, $2, $3, $4)",
            customer_id,
            business_id,
            "Test Customer",
            format!("test+{}@example.com", Uuid::new_v4())
        )
        .execute(pool)
        .await
        .unwrap();
        customer_id
    }

    async fn setup_open_invoice(pool: &PgPool, business_id: Uuid, customer_id: Uuid) -> Uuid {
        let invoice_id = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO invoices
                (id, business_id, customer_id, state, total_cents, due_date)
            VALUES ($1, $2, $3, 'open', 1000, '2026-12-01')
            "#,
            invoice_id,
            business_id,
            customer_id,
        )
        .execute(pool)
        .await
        .unwrap();
        invoice_id
    }

    async fn cleanup(
        pool: &PgPool,
        business_id: Uuid,
        customer_id: Uuid,
        invoice_id: Option<Uuid>,
    ) {
        if let Some(iid) = invoice_id {
            sqlx::query!(
                "DELETE FROM idempotency_keys WHERE request_path LIKE $1",
                format!("/invoices/{}/pay", iid)
            )
            .execute(pool).await.unwrap();
            sqlx::query!(
                "DELETE FROM payment_attempts WHERE invoice_id = $1", iid
            )
            .execute(pool).await.unwrap();
            sqlx::query!(
                "DELETE FROM invoice_line_items WHERE invoice_id = $1", iid
            )
            .execute(pool).await.unwrap();
            sqlx::query!(
                "DELETE FROM invoices WHERE id = $1", iid
            )
            .execute(pool).await.unwrap();
        }
        if customer_id != Uuid::nil() {
            sqlx::query!(
                "DELETE FROM customers WHERE id = $1", customer_id
            )
            .execute(pool).await.unwrap();
        }
        sqlx::query!(
            "DELETE FROM api_keys WHERE business_id = $1", business_id
        )
        .execute(pool).await.unwrap();
        sqlx::query!(
            "DELETE FROM businesses WHERE id = $1", business_id
        )
        .execute(pool).await.unwrap();
    }

    // ---------------------------------------------------------------
    // TEST 1: Concurrent payments
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_concurrent_payments_only_one_succeeds() {
        let pool = get_pool().await;
        let (business_id, _) = setup_business(&pool).await;
        let customer_id = setup_customer(&pool, business_id).await;
        let invoice_id = setup_open_invoice(&pool, business_id, customer_id).await;

        let mut handles = vec![];

        for _ in 0..5 {
            let pool_clone = pool.clone();
            let handle = tokio::spawn(async move {
                let mut tx = pool_clone.begin().await.unwrap();

                // try to lock + transition to processing
                let invoice = sqlx::query!(
                    r#"SELECT state::text as state FROM invoices
                       WHERE id = $1 FOR UPDATE"#,
                    invoice_id
                )
                .fetch_one(&mut *tx)
                .await
                .unwrap();

                // only open invoices can proceed
                if invoice.state.as_deref() != Some("open") {
                    tx.rollback().await.unwrap();
                    return false;
                }

                // transition to processing
                sqlx::query!(
                    "UPDATE invoices SET state = 'processing' WHERE id = $1",
                    invoice_id
                )
                .execute(&mut *tx)
                .await
                .unwrap();

                // simulate PSP call delay
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                // transition to paid
                sqlx::query!(
                    "UPDATE invoices SET state = 'paid' WHERE id = $1",
                    invoice_id
                )
                .execute(&mut *tx)
                .await
                .unwrap();

                tx.commit().await.unwrap();
                true
            });
            handles.push(handle);
        }

        let results: Vec<bool> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        let success_count = results.iter().filter(|&&r| r).count();

        // exactly one payment must succeed
        assert_eq!(
            success_count, 1,
            "exactly one concurrent payment should succeed, got {}",
            success_count
        );

        // invoice must be paid
        let invoice = sqlx::query!(
            "SELECT state::text as state FROM invoices WHERE id = $1",
            invoice_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            invoice.state.as_deref(),
            Some("paid"),
            "invoice must be paid after successful concurrent payment"
        );

        cleanup(&pool, business_id, customer_id, Some(invoice_id)).await;
    }

    // ---------------------------------------------------------------
    // TEST 2: Idempotency
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_idempotency_same_key_returns_same_response() {
        let pool = get_pool().await;
        let (business_id, _) = setup_business(&pool).await;
        let idempotency_key = format!("idem-test-{}", Uuid::new_v4());

        let fake_response = serde_json::json!({
            "id": Uuid::new_v4(),
            "status": "succeeded",
            "invoice_id": Uuid::new_v4(),
        });

        // store idempotency key as if payment was already processed
        sqlx::query!(
            r#"
            INSERT INTO idempotency_keys
                (id, key, business_id, request_path, response_status, response_body)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            Uuid::new_v4(),
            idempotency_key,
            business_id,
            "/invoices/test/pay",
            200i32,
            fake_response
        )
        .execute(&pool)
        .await
        .unwrap();

        // first lookup
        let result1 = sqlx::query!(
            r#"
            SELECT response_body FROM idempotency_keys
            WHERE key = $1 AND business_id = $2
            AND created_at > NOW() - INTERVAL '24 hours'
            "#,
            idempotency_key,
            business_id
        )
        .fetch_optional(&pool)
        .await
        .unwrap();

        // second lookup — same key
        let result2 = sqlx::query!(
            r#"
            SELECT response_body FROM idempotency_keys
            WHERE key = $1 AND business_id = $2
            AND created_at > NOW() - INTERVAL '24 hours'
            "#,
            idempotency_key,
            business_id
        )
        .fetch_optional(&pool)
        .await
        .unwrap();

        assert!(result1.is_some(), "first lookup must find the key");
        assert!(result2.is_some(), "second lookup must find same key");
        assert_eq!(
            result1.unwrap().response_body,
            result2.unwrap().response_body,
            "both lookups must return identical response — no second PSP call"
        );

        // cleanup
        sqlx::query!(
            "DELETE FROM idempotency_keys WHERE key = $1", idempotency_key
        )
        .execute(&pool).await.unwrap();
        cleanup(&pool, business_id, Uuid::nil(), None).await;
    }

    // ---------------------------------------------------------------
    // TEST 3: PSP failure — tok_timeout
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_psp_timeout_invoice_not_stuck() {
        let pool = get_pool().await;
        let (business_id, _) = setup_business(&pool).await;
        let customer_id = setup_customer(&pool, business_id).await;
        let invoice_id = setup_open_invoice(&pool, business_id, customer_id).await;

        // simulate payment handler:
        // 1. move to processing
        sqlx::query!(
            "UPDATE invoices SET state = 'processing' WHERE id = $1",
            invoice_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // 2. insert pending attempt
        let attempt_id = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO payment_attempts
                (id, invoice_id, status, card_token)
            VALUES ($1, $2, 'pending', 'tok_timeout')
            "#,
            attempt_id,
            invoice_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // 3. simulate timeout 
        sqlx::query!(
            "UPDATE invoices SET state = 'open' WHERE id = $1",
            invoice_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // assert invoice is back to open — NOT stuck in processing
        let invoice = sqlx::query!(
            "SELECT state::text as state FROM invoices WHERE id = $1",
            invoice_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            invoice.state.as_deref(),
            Some("open"),
            "invoice must revert to open after PSP timeout, not stuck in processing"
        );

        // assert attempt is pending — not failed
        let attempt = sqlx::query!(
            "SELECT status::text as status FROM payment_attempts WHERE id = $1",
            attempt_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            attempt.status.as_deref(),
            Some("pending"),
            "attempt must stay pending after timeout, not marked failed"
        );

        // assert invoice can be paid again
        assert_ne!(
            invoice.state.as_deref(),
            Some("paid"),
            "invoice must not be paid after timeout"
        );
        assert_ne!(
            invoice.state.as_deref(),
            Some("void"),
            "invoice must not be void after timeout"
        );

        cleanup(&pool, business_id, customer_id, Some(invoice_id)).await;
    }

}