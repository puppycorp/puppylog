#!/usr/bin/env python3
"""
Generate synthetic log lines without angle bracket tags, using a cyclical order.
"""

import argparse
import random
import string
import requests
import gzip
from io import BytesIO
from datetime import datetime, timedelta

# Possible log levels and their relative frequencies.
LOGLEVELS = ["DEBUG", "INFO", "WARN", "ERROR", "CRITICAL"]

# Entities and their corresponding actions.
ENTITY_TYPES = [
    "instance", "user", "service", "device", "transaction", "task", "api request",
    "container", "node", "backup", "scheduler job", "email", "cache",
    "webhook", "database", "notification", "deployment", "license",
    "analytics event", "report", "session", "payment"
]

ACTIONS = {
    "instance": ["created", "updated", "deleted"],
    "user": ["registered", "logged in", "logged out unexpectedly"],
    "service": ["started", "latency detected", "crashed"],
    "device": ["connected", "signal weak", "disconnected"],
    "transaction": ["initiated", "processed", "failed"],
    "task": ["created", "running", "completed"],
    "api request": ["initiated", "returned status", "failed"],
    "container": ["started", "resource high", "crashed"],
    "node": ["joined cluster", "under heavy load", "removed from cluster"],
    "backup": ["started", "completed", "failed"],
    "scheduler job": ["scheduled", "executing", "finished"],
    "email": ["sent to", "delivery delayed to", "bounced from"],
    "cache": ["cleared", "hit rate recorded", "updated"],
    "webhook": ["received from", "processed successfully", "processing failed"],
    "database": ["connection established", "query slow", "connection lost"],
    "notification": ["queued for user", "delivered to user", "failed to deliver to user"],
    "deployment": ["initiated by user", "in progress", "aborted due to error"],
    "license": ["activated for user", "nearing expiration for user", "renewed for user"],
    "analytics event": ["recorded for user", "processed", "failed to process"],
    "report": ["generated for user", "downloaded by user", "generation failed for user"],
    "session": ["started for user", "active", "inactive for too long"],
    "payment": ["initiated by user", "authorized", "declined for user"]
}

# Miscellaneous data sources for placeholders.
STATUS_CODES = [200, 201, 400, 401, 403, 404, 500, 502, 503]
EMAIL_DOMAINS = ["example.com", "mail.com", "test.org", "sample.net"]
SERVICE_NAMES = ["AuthService", "DataService", "PaymentService", "NotificationService"]
DEVICE_NAMES = ["DeviceA", "DeviceB", "SensorX", "SensorY"]
API_NAMES = ["GetUser", "CreateOrder", "UpdateProfile", "DeleteAccount"]
DATABASE_NAMES = ["UserDB", "OrderDB", "AnalyticsDB", "InventoryDB"]
WEBHOOK_SOURCES = ["GitHub", "Stripe", "Slack", "Twilio"]
LICENSE_TYPES = ["Pro", "Enterprise", "Basic", "Premium"]
REPORT_TYPES = ["Sales", "Inventory", "UserActivity", "Performance"]

# Functions to generate various random IDs.
def generate_session_id():
    return ''.join(random.choices(string.ascii_letters + string.digits, k=12))

def generate_transaction_id():
    return ''.join(random.choices(string.ascii_uppercase + string.digits, k=10))

def generate_task_id():
    return ''.join(random.choices(string.ascii_lowercase + string.digits, k=8))

def generate_deployment_id():
    return 'deploy-' + ''.join(random.choices(string.digits, k=6))

def generate_license_id():
    return 'lic-' + ''.join(random.choices(string.ascii_uppercase + string.digits, k=8))

def generate_analytics_event_id():
    return 'evt-' + ''.join(random.choices(string.ascii_lowercase + string.digits, k=10))

def generate_report_id():
    return 'rep-' + ''.join(random.choices(string.ascii_uppercase + string.digits, k=7))

def generate_payment_id():
    return 'pay-' + ''.join(random.choices(string.ascii_uppercase + string.digits, k=9))

def random_string_name():
    """Generate a random name-like string."""
    return ''.join(random.choices(string.ascii_letters, k=random.randint(5, 10)))

def random_email():
    """Generate a random email address."""
    return f"{''.join(random.choices(string.ascii_lowercase, k=7))}@{random.choice(EMAIL_DOMAINS)}"

def random_timestamp(base_time):
    """Generate a random timestamp close to base_time in ISO format."""
    offset_seconds = random.randint(0, 100000)
    ts = (base_time + timedelta(seconds=offset_seconds)).isoformat()
    return ts

def generate_log_line(order, base_time):
    """
    Generate a single log line without angle-bracket tags.
    `order` cycles 1 -> 2 -> 3.
    """
    # Pick a loglevel, entity, and action.
    loglevel = random.choices(LOGLEVELS, weights=[5, 50, 30, 10, 5])[0]
    entity = random.choice(ENTITY_TYPES)
    action = random.choice(ACTIONS[entity])

    # Generate placeholders.
    placeholders = {}
    placeholders["loglevel"] = loglevel
    placeholders["entity"] = entity
    placeholders["action"] = action
    placeholders["timestamp"] = random_timestamp(base_time)

    # Decide how to fill in entity-specific fields/IDs.
    if entity == "api request":
        # e.g.: "returned status 404"
        placeholders["api_name"] = random.choice(API_NAMES)
        placeholders["status_code"] = str(random.choice(STATUS_CODES))
    elif entity == "service":
        placeholders["service_name"] = random.choice(SERVICE_NAMES)
    elif entity == "device":
        placeholders["device_name"] = random.choice(DEVICE_NAMES)
    elif entity == "webhook":
        placeholders["webhook_source"] = random.choice(WEBHOOK_SOURCES)
    elif entity == "database":
        placeholders["database_name"] = random.choice(DATABASE_NAMES)
    elif entity == "notification":
        placeholders["notification_id"] = ''.join(random.choices(string.ascii_letters + string.digits, k=8))
    elif entity == "deployment":
        placeholders["deployment_id"] = generate_deployment_id()
    elif entity == "license":
        placeholders["license_id"] = generate_license_id()
    elif entity == "analytics event":
        placeholders["analytics_event_id"] = generate_analytics_event_id()
    elif entity == "report":
        placeholders["report_id"] = generate_report_id()
    elif entity == "payment":
        placeholders["payment_id"] = generate_payment_id()
    elif entity == "transaction":
        placeholders["transaction_id"] = generate_transaction_id()
    elif entity == "task":
        placeholders["task_id"] = generate_task_id()
    elif entity == "session":
        placeholders["session_id"] = generate_session_id()
    elif entity == "user":
        placeholders["user_name"] = random_string_name()
        placeholders["user_email"] = random_email()

    # Fallback for ID-based entities (like "instance", "backup", etc.) if needed.
    # If no special ID, generate a generic one:
    if entity not in ["user", "service", "device", "api request", "webhook", "database",
                      "notification", "deployment", "license", "analytics event", "report",
                      "payment", "transaction", "task", "session"]:
        placeholders["generic_id"] = ''.join(random.choices(string.ascii_letters + string.digits, k=8))

    # Construct the log line string without angle brackets.
    # We'll incorporate `order` for sequence, but without `<ORDER: x>`.
    
    # Examples of final formatting:
    #   INFO user JohnDoe registered at 2025-01-06 12:34:56, order=1
    #   WARN api request UpdateProfile returned status 404 at 2025-01-06 12:34:56, order=2

    # The pattern is: {loglevel} {entity} [entity_identifier] {action} [extra details?] at {timestamp}
    # We'll tailor the [entity_identifier] and [extra details] based on the entity + action.

    if entity == "user":
        line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['user_name']} {action}"
    elif entity == "api request":
        if action == "returned status":
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['api_name']} returned status {placeholders['status_code']}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['api_name']} {action}"
    elif entity == "service":
        line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['service_name']} {action}"
    elif entity == "device":
        line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['device_name']} {action}"
    elif entity == "webhook":
        if action == "received from":
            line = f"{placeholders['timestamp']} {loglevel} {entity} {action} {placeholders['webhook_source']}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {action}"
    elif entity == "database":
        line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['database_name']} {action}"
    elif entity == "notification":
        if "user" in action:
            user_for_notification = random_string_name()
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['notification_id']} {action} {user_for_notification}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['notification_id']} {action}"
    elif entity == "deployment":
        if "by user" in action:
            random_user = random_string_name()
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['deployment_id']} {action} {random_user}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['deployment_id']} {action}"
    elif entity == "license":
        if "for user" in action:
            random_user = random_string_name()
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['license_id']} {action} {random_user}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['license_id']} {action}"
    elif entity == "analytics event":
        if "for user" in action:
            random_user = random_string_name()
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['analytics_event_id']} {action} {random_user}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['analytics_event_id']} {action}"
    elif entity == "report":
        if "for user" in action:
            random_user = random_string_name()
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['report_id']} {action} {random_user}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['report_id']} {action}"
    elif entity == "payment":
        if "by user" in action:
            random_user = random_string_name()
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['payment_id']} {action} {random_user}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['payment_id']} {action}"
    elif entity == "transaction":
        line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['transaction_id']} {action}"
    elif entity == "task":
        line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['task_id']} {action}"
    elif entity == "session":
        if "for user" in action:
            random_user = random_string_name()
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['session_id']} {action} {random_user}"
        else:
            line = f"{placeholders['timestamp']} {loglevel} {entity} {placeholders['session_id']} {action}"
    else:
        generic_id = placeholders.get("generic_id", "")
        line = f"{placeholders['timestamp']} {loglevel} {entity} {generic_id} {action}"

    return line

def generate_logs(total_logs):
    """Generate `total_logs` lines, cycling order from 1 to 2 to 3."""
    logs = []
    base_time = datetime.now()
    for i in range(1, total_logs + 1):
        order = (i % 3) + 1  # cycles through 1, 2, 3
        line = generate_log_line(order, base_time)
        logs.append(line)
    return logs

def main():
    parser = argparse.ArgumentParser(
        description="Generate synthetic log lines for anomaly detection without angle-bracket tags."
    )
    parser.add_argument(
        "--count", "-c", type=int, default=1000,
        help="Number of log lines to generate (default: 1000)"
    )
    # parser.add_argument(
    #     "--output", "-o", type=str, default="generated_logs.txt",
    #     help="Output file to store generated logs (default: generated_logs.txt)"
    # )
    subparsers = parser.add_subparsers(dest="command")
    rawupload = subparsers.add_parser("rawupload")
    rawupload.add_argument(
        "address", type=str,
        help="Server URL to upload raw logdata (example: http://localhost:8080)"
    )

    args = parser.parse_args()

    print(args)

    logs_str = "\n".join(generate_logs(args.count))

    if args.command == "rawupload":
        print(f"Uploading raw log data to {args.address}...")

        # Compress the log data
        buf = BytesIO()
        with gzip.GzipFile(fileobj=buf, mode='wb') as f:
            f.write(logs_str.encode('utf-8'))
        
        compressed_data = buf.getvalue()
        
        # Send the compressed data
        headers = {
            #"Content-Encoding": "gzip",
        }
        res = requests.post(args.address, data=logs_str, headers=headers)
        
        print(res.status_code)


    # log_entries = generate_logs(args.count)
    # with open(args.output, "w") as f:
    #     for entry in log_entries:
    #         f.write(entry + "\n")

    # print(f"Generated {args.count} log lines in {args.output}.")

if __name__ == "__main__":
    main()
