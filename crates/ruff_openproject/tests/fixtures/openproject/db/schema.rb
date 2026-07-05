ActiveRecord::Schema.define(version: 2026_06_01_000000) do
  create_table "work_packages", force: :cascade do |t|
    t.string   "subject", null: false
    t.text     "description"
    t.integer  "status_id"
    t.string   "status"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end

  create_table "time_entries", force: :cascade do |t|
    t.integer "work_package_id"
    t.integer "user_id"
    t.float   "hours"
    t.datetime "spent_on"
    t.index ["work_package_id"], name: "index_time_entries_on_work_package_id"
    t.index ["user_id"], name: "index_time_entries_on_user_id"
    t.foreign_key "work_packages", column: "work_package_id"
  end
end
